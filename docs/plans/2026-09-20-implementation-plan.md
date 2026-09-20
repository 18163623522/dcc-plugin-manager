# DCC Plugin Manager 实现计划（里程碑版）

> **执行方式：** 本文档是任务级路线图（精确到文件/接口/验收）。实际动工每个里程碑时，
> 按当前代码现状把对应任务展开为逐步 TDD 执行清单（writing-plans 流程），避免预写代码漂移。
> 计划日期：2026-09-20。设计依据：`docs/specs/2026-09-20-dcc-plugin-manager-design.md`（权威）。

**Goal:** Windows 桌面应用：检测本机 UE / Houdini，从 GitHub 仓库或本地目录一键安装/更新/卸载 DCC 插件，Release 优先、源码 RunUAT 构建兜底。

**Architecture:** Tauri 2 壳；前端 Vue3 + Vite + TS 只做 UI；Rust 后端按单一职责拆 8 个模块（detect / sources / registry / install / update / uninstall / compat / preflight），经 Tauri command 暴露给前端；持久化只有 `%APPDATA%\dcc-plugin-manager\registry.json` 与缓存目录。

**Tech Stack:** Tauri 2、Vue 3 + Vite + TypeScript、Rust（std + serde + git2 或 shell 出 git）、obs-float-bar 设计 token + beautifului/winui3 参考（见 ui-reference/）。

## Global Constraints

- 仅支持 Windows（Win64）；路径处理一律用 Rust `PathBuf`，不拼字符串。
- UI 视觉基调 obs-float-bar（设计文档 §6，冲突时优先）；等宽字体用于日志/版本号，`tabular-nums`。
- 长操作（clone/构建/下载）必须流式日志到 UI（Tauri event），禁止静默。
- 删除操作只允许删 registry 记录过的精确路径；本地源目录永不被删。
- Rust 单元测试不依赖真机也能跑：引擎解析用 fixtures 快照（`src-tauri/tests/fixtures/`）。
- 提交信息沿用仓库中文约定式风格（`feat(scope): ...`）。

---

## 文件结构总览（锁定分解）

```
dcc-plugin-manager/
├─ src/                          # Vue 前端
│  ├─ main.ts  App.vue
│  ├─ styles/tokens.css          # obs-float-bar 设计 token（§6）→ CSS 变量
│  ├─ components/                # GlowButton.vue StatusDot.vue EngineChip.vue
│  │  ├─ PluginList.vue          # 插件列表（filter-table 形态）
│  │  ├─ FilterBar.vue           # 分类 chip + 搜索（§4.1）
│  │  ├─ InstallDialog.vue       # 引擎多选 + 兼容徽标
│  │  ├─ LogPanel.vue            # 构建日志流（code-block 形态）
│  │  └─ ConfirmDialog.vue       # 卸载确认（approval-card 形态）
│  └─ views/{Plugins,Engines,Settings}.vue
├─ src-tauri/src/
│  ├─ main.rs                    # Tauri 壳 + command 注册
│  ├─ detect/{mod.rs,ue.rs,houdini.rs}      # 引擎/宿主检测
│  ├─ sources/{mod.rs,git.rs,local.rs}      # 来源管理
│  ├─ registry.rs                # registry.json 读写 + 数据类型
│  ├─ install/{mod.rs,release.rs,copy.rs,build.rs}  # 安装流水线
│  ├─ update.rs                  # 更新检查
│  ├─ uninstall.rs               # 卸载器
│  ├─ compat.rs                  # 版本兼容矩阵（§3.7）
│  ├─ preflight.rs               # 安装前诊断（§3.8）
│  └─ commands.rs                # #[tauri::command] 层（薄，只转发+事件）
├─ src-tauri/tests/fixtures/     # LauncherInstalled.dat 快照、样例 uplugin
└─ ui-reference/                 # 已入库（beautifului + winui3）
```

**数据类型契约（registry.rs，其他模块依赖此签名）：**

```rust
pub struct PluginEntry {
    pub id: String,                       // uplugin Name 或目录名
    pub kind: PluginKind,                 // UE | Houdini
    pub source: PluginSource,             // Git{url, default_ref} | Local{path}
    pub version: String,
    pub commit: Option<String>,
    pub installed: Vec<InstalledTarget>,
}
pub struct InstalledTarget {
    pub engine: String,                   // "UE-5.7.3" / "Houdini-20.5"
    pub path: PathBuf,
    pub method: InstallMethod,            // Release | Build | BinaryCopy
    pub installed_at: String,             // RFC3339
}
```

---

## 里程碑 1：骨架 + 引擎检测 + 本地源 + 手动安装/卸载闭环

**Definition of Done：** 应用能列出本机全部 UE 引擎与 Houdini；添加本地插件目录 →
勾选引擎 → 拷贝安装到 `Engine\Plugins\Marketplace\<Name>` → 列表显示 → 按引擎卸载干净。

### Task 1.1 Tauri 2 工程骨架 + 无边框毛玻璃壳
- Create: `src-tauri/`（tauri.conf.json decorations:false transparent:true）、`src/`、`src/styles/tokens.css`
- 步骤：`pnpm create tauri-app`（vue-ts 模板）→ 抄 obs-float-bar 的 `#shell` 样式进 tokens.css
  （rgba(22,22,24,.88) / blur(24px) / 1px 8%白描边 / 圆角16px）→ 自绘 header 拖动区 `data-tauri-drag-region`。
- 验收：`pnpm tauri dev` 弹出 960×640 毛玻璃窗口，可拖动，GitHub push。

### Task 1.2 detect::ue —— LauncherInstalled.dat 解析
- Create: `src-tauri/src/detect/ue.rs`；Test: `src-tauri/tests/ue_detect.rs`
- 接口：`pub fn detect_ue_engines(extra_roots: &[PathBuf]) -> Vec<UeEngine>`，
  `UeEngine { version: String /*"5.7.3"*/, root: PathBuf, runuat: PathBuf }`
- 逻辑：解析 `C:\ProgramData\Epic\UnrealEngineLauncher\LauncherInstalled.dat`（JSON），
  按 InstallLocation 去重 → 版本号取 `AppVersion` 前三段最大者 → 校验 `Engine\Build\BatchFiles\RunUAT.bat` 存在。
- 测试：fixtures 放本机真实 dat 快照（含 5.7/5.8/多个 Marketplace 条目），断言去重后版本数与 RunUAT 路径。
- 验收：`cargo test ue_detect` 通过； Engines 页显示 7 个引擎。

### Task 1.3 detect::houdini —— 注册表扫描
- Create: `src-tauri/src/detect/houdini.rs`
- 接口：`pub fn detect_houdini() -> Vec<HoudiniInstall>`，含 `version`、`packages_dir`（`%USERPROFILE%\Documents\houdini<major.minor>\packages`，尊重 `HOUDINI_USER_PREF_DIR`）。
- 实现：`winreg` 遍历 `HKLM\SOFTWARE\Side Effects Software\Houdini\*` + 目录存在性校验。
- 验收：`cargo test houdini_detect`（用注入的假注册表键或抽象 trait 测）；本机列出已装版本。

### Task 1.4 registry 模块
- Create: `src-tauri/src/registry.rs`；Test: `src-tauri/tests/registry.rs`
- 接口：`load(app_dir)/save/add_or_update/remove_target(path)`；损坏 JSON → 备份为 `registry.json.bad.<ts>` 后重建空表。
- 验收：单测覆盖 读写往返/升级字段缺省/损坏恢复 三例。

### Task 1.5 sources::local —— 本地目录源识别
- Create: `src-tauri/src/sources/local.rs`
- 接口：`pub fn inspect_local(path) -> LocalPlugin`：目录内递归（深度≤2）找 `*.uplugin` → UE 插件
  （读 Name/VersionName/EngineVersion，JSON 解析容错 BOM）；无 uplugin 但含 `*.otls|*.hda|hda/*.dll` 约定 → Houdini；都不满足 → 报"未识别插件结构"。
- 验收：fixtures 三个目录（真 uplugin / houdini 包 / 空）单测通过。

### Task 1.6 install::copy —— 二进制手动安装
- Create: `src-tauri/src/install/copy.rs`
- 接口：`pub fn install_binary(src: &Path, engine: &UeEngine) -> Result<InstalledTarget, InstallError>`
- 逻辑：目标 `<root>\Engine\Plugins\Marketplace\<Name>`；已存在 → 先删（文件锁检测：尝试以独占打开任一 dll，失败返回 `InstallError::Locked(process)`）；整目录拷贝后写 registry。
- 验收：对 `F:\Cache\AI\TrueGlow-Build58` 手动安装到测试用引擎目录，断言文件数一致、registry 记录正确；锁场景用被占用文件模拟。

### Task 1.7 uninstall —— 按引擎卸载
- Create: `src-tauri/src/uninstall.rs`
- 接口：`pub fn uninstall(entry_id, engines: &[String], keep_cache: bool) -> UninstallReport`
- 规则：只删 `InstalledTarget.path` 精确路径；删前列出路径清单（前端确认框展示）；锁 → 中止该项并报告；全部卸载后视 keep_cache 决定是否删 git 缓存（本地源永不删源）。
- 验收：安装→卸载往返测试；伪造 registry 路径不存在 → 报告 missing 而非崩溃。

### Task 1.8 前端：插件列表 + 分类搜索 v1 + 添加本地源
- Create: `src/views/Plugins.vue`、`src/components/{PluginList,FilterBar,ConfirmDialog}.vue`、`src-tauri/src/commands.rs`（command 层一次性建好：detect/list/add_local/install/uninstall）
- 交互按 §4.1：宿主/状态/来源 chip（引擎维度 M2 接 compat 后加）+ 模糊搜索；行内操作 安装/卸载/打开目录。
- 验收：手动全流程（添加 TrueGlow-Build58 本地目录 → 装 5.8 → 列表绿点 → 卸载）走通；UI 对照 tokens.css。

## 里程碑 2：GitHub 源 + Release 安装 + 更新检查 + 兼容矩阵

### Task 2.1 sources::git —— clone/pull + gh 可用性
- Create: `src-tauri/src/sources/git.rs`
- 接口：`ensure_repo(url, cache_dir) -> RepoState{path, head, default_ref}`（存在则 `git pull --ff-only`）；
  `list_refs(url) -> Vec<RefInfo>`（gh api 优先，`gh` 不在则 `git ls-remote`）。
- 进度经 `tauri::Emitter` 发 `install-log` 事件。缓存：`%APPDATA%\dcc-plugin-manager\cache\repos\<name>`。
- 验收：对 TrueGlow 仓库 clone→二次 pull 秒回；断网时本地源功能不受影响。

### Task 2.2 compat —— 版本兼容矩阵（§3.7 四层信号）
- Create: `src-tauri/src/compat.rs`；Test: `src-tauri/tests/compat.rs`
- 分支解析正则（已实测用户仓库命名）：`(?i)ue[-_]?(\d)(\d)`（`ue5.7`/`ue4.26`）、
  裸 `ue5`/`ue4` 家族、容忍后缀（`ue5.7-complete`）、仓库名 `_UE(\d)_(\d)` 弱提示；
  候选 ref 拉 `.uplugin`（raw）读 EngineVersion 做权威确认。
- 输出：`compat(plugin, engines) -> Vec<(engine, Status)>`，Status = Installed | Installable{ref} | Unverified | Incompatible{reason}。
- 测试：用 fixtures 覆盖 TrueGlow（ue5.7/ue5.8 分支）、WireShakeBreak（ue5）、MatHelper_UE426（仅默认分支）、BetterHLSL（噪音分支）四型。
- 验收：`cargo test compat`；前端 InstallDialog 引擎徽标四态正确。

### Task 2.3 install::release —— Release 附件安装
- Create: `src-tauri/src/install/release.rs`
- 逻辑：gh api 取 latest release → 选含 zip 的 asset → 下载到缓存 → 解压探测 `.uplugin`（根或一层嵌套）→ 复用 install::copy；无匹配 asset 返回 `NoReleaseAsset`（前端引导走 M3 构建）。
- 验收：对带 Release 的仓库（临时造一个测试 release 或用现有公开仓库）走通下载安装。

### Task 2.4 update —— 更新检查
- Create: `src-tauri/src/update.rs`
- git 源：latest release tag / 默认分支 HEAD vs registry.version/commit；本地源：目录 mtime+size 摘要变化。
- 验收：构造旧版本 registry 记录 → 列表出现黄点；本地源改动文件 → 黄点。

## 里程碑 3：RunUAT 构建流水线 + preflight 诊断

### Task 3.1 preflight —— 五项预检（§3.8）
- Create: `src-tauri/src/preflight.rs`；Test: `src-tauri/tests/preflight.rs`
- 检查项与文案照设计表格；VS 工具链检测：`vswhere -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64`。
- 验收：单测覆盖每项的失败分支文案；前端禁用态按钮 tooltip 显示原因。

### Task 3.2 install::build —— RunUAT BuildPlugin 流水线
- Create: `src-tauri/src/install/build.rs`
- 命令（本会话已验证可行）：`<engine>\Engine\Build\BatchFiles\RunUAT.bat BuildPlugin -Plugin=<uplugin> -Package=<cache>\build\<ver> -TargetPlatforms=Win64 -NoHostPlatformHeaders`
- stdout 逐行 `install-log` 事件；exit code ≠0 → 高亮 error 行、不写 registry；成功 → 复用 copy 落位。
- 验收：TrueGlow 源码对 5.7 全流程构建安装成功（约 1-15 分钟，日志流可见）；中途取消能杀进程树。

### Task 3.3 试编译裁决（未验证引擎）
- Modify: `compat.rs` 增加 `try_build` 路径：Unverified 状态下用户点"尝试编译"→ 走 3.2，成功回写 registry 并标 Installable-verified。
- 验收：对无版本分支的插件在 5.6 上试编译，失败信息可读，成功后徽标转绿。

## 里程碑 4：Houdini 支持

### Task 4.1 Houdini 安装 + packages json 生成
- Create: `src-tauri/src/install/houdini.rs`
- 逻辑：拷插件目录到 `<packages_dir>\..\..\..\Documents\houdini<ver>\plugins\<name>`（随 pref dir 计算），
  生成 `packages\<name>.json`：`{"env":[{"HOUDINI_PATH":"<插件路径>&"}],"path":"<插件路径>"}`；
  已存在同名包 → 覆盖前确认。
- 验收：样例 otls 包安装后 Houdini 能在 File>… 或 hscript `packages` 可见路径；重复安装幂等。

### Task 4.2 Houdini 卸载 + 更新
- Modify: `uninstall.rs`/`update.rs` 覆盖 Houdini 分支（删插件目录+json；更新 = 重装）。
- 验收：装→卸→装的往返与 registry 状态一致。

## 里程碑 5：Obsidian 插件管理（2026-09-20 追加）

设计依据：`docs/specs/...design.md` §8.1（vault=引擎映射表）。复用 sources/registry/update/uninstall 骨架，新增 detect 与 install 的 Obsidian 分支。

### Task 5.1 detect::obsidian —— vault 枚举
- Create: `src-tauri/src/detect/obsidian.rs`；Test: fixtures obsidian.json 快照
- 接口：`detect_vaults() -> Vec<ObsidianVault>`：解析 `%APPDATA%\obsidian\obsidian.json` 的
  `vaults`（id→path，含 `open` 标记当前库）+ 路径存在性校验；应用版本读取供 minAppVersion 兼容。
- registry 语义：`InstalledTarget.engine = "Obsidian@<vault名>"`。

### Task 5.2 sources 识别 manifest.json + install::obsidian Release 散件
- Modify: `sources/local.rs`（识别 manifest.json → Obsidian 插件：id/version/minAppVersion）
- Create: `src-tauri/src/install/obsidian.rs`
  - Release 散件收集：main.js + manifest.json（必需）+ styles.css（可选）三个独立附件 → 下载到缓存
    （install::release 扩展非 zip 资产路径）；zip 附件走既有解压探测
  - 落位：`<vault>\.obsidian\plugins\<id>\`（已存在 → 整目录替换；Obsidian 运行中文件被锁 → Lock 报错提示）
  - 完成提示"在 Obsidian 设置 → 第三方插件中启用"（不写 community-plugins.json，避免竞态）
- 验收：对带标准 Release 散件的公开插件仓库走通装→列表→卸载。

### Task 5.3 源码构建兜底 + compat（minAppVersion）
- Create: `install/obsidian.rs` 构建路径：`pnpm install && pnpm run build`（shell，CREATE_NO_WINDOW，
  日志流 install-log）→ 产物取 main.js/manifest.json/styles.css
- compat：manifest `minAppVersion` vs 应用版本 → ✅/❌（isDesktopOnly 仅提示）
- 验收：本机 obsidian-canvas-plus 等私有仓库（gh 已登录可 clone）走源码构建安装成功。

### Task 5.4 前端
- 引擎页 vault 分组；安装对话框 vault 多选 + minAppVersion 徽标；宿主筛选 chip 加 Obsidian。
- 验收：真机 vault 列表 + 双 vault 安装。

## 里程碑 6：收尾完整化（2026-09-20 追加，M1–M5 完成后）

设计文档中尚未落地的交互补齐 + 可交付打包。

### Task 6.1 设置页落地
- Create: `src-tauri/src/settings.rs`（`settings.json`：`{extra_ue_roots: []}`，损坏容错同 registry）
- Commands: `get_settings`/`save_settings`（自定义 UE 引擎根目录，detect_engines 读取生效）、
  `cache_stats`（repos/releases/build 三目录大小）、`clear_cache(kinds)`（只删 cache 子目录，本地源不受影响）、
  `env_status`（gh / curl / pnpm 可用性）。
- Settings.vue：缓存三行 + 清理按钮（显示将释放空间）；引擎根目录列表增删；环境徽标。

### Task 6.2 列表引擎筛选维度（设计 §4.1 遗留）
- FilterBar 增加引擎 chip 组（检测到的 UE 版本 + Houdini 版本 + vault 名动态生成）；
  选中某引擎 → 列表只显示已装该引擎的插件；兼容徽标在该维度下简化为单态。

### Task 6.3 打包
- `pnpm tauri build`（release 无控制台黑窗），产物路径报告。
- 验收：release exe 双击可用，全流程不依赖 dev server。

---

## 验证策略（贯穿）

- 每个 Rust 模块 `cargo test`；fixtures 全部来自本机真实文件快照（LauncherInstalled.dat、TrueGlow.uplugin 等）。
- 每个里程碑结束跑一次端到端手动清单（TrueGlow 本地源 / git 源双路径），记录进提交信息。
- UI 验收对照 §6 token 清单（毛玻璃参数、语义色、tabular-nums、36px 圆钮）。

## 里程碑与仓库分支策略

`main` 持续可发布；每个里程碑一个 `feat/m<N>` 分支，完成合回。GitHub 网络不通时用 `push-api.py`
（Git Data API 推送，已验证 SHA 同源）。
