# DCC Plugin Manager 设计文档

日期：2026-09-20
状态：待用户评审
工作名：dcc-plugin-manager（可改）

## 1. 目标与背景

用户维护大量自研 Unreal Engine 插件（GitHub 仓库为主），未来还会做 Houdini 插件。当前安装/更新流程手工繁琐（克隆 → 按引擎 RunUAT BuildPlugin → 拷入引擎插件目录）。本软件把全流程自动化：

- 一键安装（自动检测引擎、下载或编译、落位）
- 版本可见（已装版本 vs 来源最新版本）
- 更新与卸载（含按引擎粒度）
- 支持 UE 与 Houdini 双生态

非目标（v1 明确不做）：UE 单工程级安装（只装引擎全局）、插件市场浏览、自动解决跨版本源码移植（如 4.26→5.7 API 改造仍需人工，软件只负责按引擎构建安装）。

## 2. 已确认的决策

| 决策点 | 结论 |
|---|---|
| 技术栈 | Tauri 2（前端 Vue3 + Vite + TS，后端 Rust） |
| v1 范围 | UE + Houdini 同时支持 |
| 插件来源 | GitHub 仓库 + 本地目录 |
| 更新来源 | GitHub Release 附件优先，无附件则源码构建兜底 |
| UE 安装位置 | 引擎全局 `Engine\Plugins\Marketplace\<Name>` |
| UI 风格 | 沿用 obs-float-bar 设计语言（见 §6） |

## 3. 架构

```
Tauri 2 壳
├─ 前端（Vue3+Vite+TS）
│   ├─ 插件列表页（已装/可装、状态徽标、行内操作）
│   ├─ 添加源（GitHub URL / 本地目录选择）
│   ├─ 安装对话框（引擎多选 + 进度日志流）
│   ├─ 引擎页（检测到的 UE/Houdini 版本）
│   └─ 设置页（缓存目录、gh 路径、清理缓存）
└─ Rust 后端（模块单一职责）
    ├─ detect   引擎/宿主检测
    ├─ sources  来源管理（git/gh/本地目录）
    ├─ registry 插件注册表（%APPDATA% JSON）
    ├─ install  安装流水线（Release/构建/落位）
    ├─ update   更新检查器
    └─ uninstall 卸载器
```

### 3.1 detect（检测器）

- **UE**：解析 `C:\ProgramData\Epic\UnrealEngineLauncher\LauncherInstalled.dat`（多引擎去重出版本号 4.26/5.1/…/5.8 与根目录）+ 设置页可追加自定义路径（`Engine\Build\BatchFiles\RunUAT.bat` 存在性校验）。
- **Houdini**：注册表 `HKLM\SOFTWARE\Side Effects Software\Houdini\*` + 标准目录扫描，得到版本与用户 packages 目录（`%USERPROFILE%\Documents\houdini<major.minor>\packages`，尊重 `HOUDINI_USER_PREF_DIR` 覆盖）。

### 3.2 sources（来源管理）

- GitHub 源：`git clone/pull` 到缓存目录 `%APPDATA%\dcc-plugin-manager\cache\repos\<name>`；Release/Tag 查询用 `gh` CLI（已登录则免 token 配置），`gh` 不存在时回退匿名 GitHub REST。
- 本地目录源：仅登记路径引用，不做复制；插件元数据（uplugin/package.json）就地读取。

### 3.3 registry（注册表）

`%APPDATA%\dcc-plugin-manager\registry.json`，结构：

```jsonc
{
  "plugins": [{
    "id": "TrueGlow",
    "type": "UE" | "Houdini",
    "source": { "kind": "git", "url": "...", "ref": "ue5.8" } | { "kind": "local", "path": "..." },
    "version": "0.8.0",              // .uplugin VersionName / package.json / Release tag
    "commit": "8ada112...",          // git 源记录，更新比对用
    "installed": [{
      "engine": "UE-5.7.3",          // 或 "Houdini-20.5"
      "path": "E:\\...\\Marketplace\\TrueGlow",
      "method": "release" | "build" | "binary-copy",
      "installedAt": "2026-09-20T05:40:00+08:00"
    }]
  }]
}
```

### 3.4 install（安装流水线）

1. **取件**：Release 有匹配附件（zip 内 `.uplugin` 在根或一层嵌套，自动探测）→ 下载解压到缓存；否则确保源码（clone/pull 或本地引用）。
2. **识别**：读 `.uplugin`（Name/VersionName/EngineVersion）或 Houdini 包结构（目录内含 `*.json`/`otls` 等约定）。识别失败 → 报错不安装。
3. **目标选择**：按 EngineVersion 提示兼容引擎，用户勾选（默认勾选兼容项）。
4. **产物**：无可用二进制 → 对每个勾选引擎跑 `RunUAT.bat BuildPlugin -Plugin=... -Package=... -TargetPlatforms=Win64`（stdout 实时流式到 UI；预检 VS 工具链与引擎完整性）。
5. **落位**：UE → 原子替换 `Engine\Plugins\Marketplace\<Name>`（先删旧目录，文件被锁则中止并提示）；Houdini → 拷插件目录到约定位置 + 生成 `packages\<name>.json`。
6. **登记**：写回 registry。

### 3.5 update（更新检查）

- git 源：远端 Release 最新 tag 或默认分支 HEAD commit vs registry 记录。
- 本地源：目录内容 hash/mtime 变化即"可更新"（重装语义）。
- 列表页徽标 + 一键更新（复用安装流水线）。

### 3.6 uninstall（卸载器）

- 粒度：按引擎单卸 / 整插件全卸。
- 删除对象：registry 中记录的精确路径（UE 目录 / Houdini 插件目录 + packages json）。
- 确认对话框列出将删除的全部路径；文件锁检测（编辑器占用 DLL）→ 明确提示关闭对应引擎后中止。
- 安全边界：本地源目录永不被删；GitHub 缓存仓库默认保留（重装免 clone），设置页可清。

### 3.7 compat（版本兼容矩阵）

回答"这个插件有没有对应我某个引擎的版本"。信号按可信度排序：

1. **已安装记录**：该引擎装过 → ✅ 已验证兼容。
2. **Git 分支/标签匹配**：扫描仓库 refs，解析版本（覆盖实测命名习惯——
   `ue5.7` 精确、`ue5` 家族泛匹配、`ue5.7-complete` 容忍后缀噪音、
   仓库名 `_UE426` 弱提示）；对候选 ref 拉取 `.uplugin` 的 EngineVersion 做权威确认。
3. **默认分支 EngineVersion**：唯一版本源声明（如 MatHelper_UE426 只声明 4.26）
   → 其他引擎标 ❌ 不兼容（提示需移植）。
4. **无任何信号** → ⚠️ 未验证，提供"尝试为此引擎编译"（BuildPlugin 成败即最终裁决，
   成功后记入 registry 为已验证）。

UI：插件行引擎徽标四态（✅已装 / ✅可装(来源分支名) / ⚠️未验证 / ❌不兼容(原因)）。

### 3.8 preflight（安装前诊断）

回答"为什么不能装"。对每个 (插件, 引擎) 组合跑预检流水线，产出可操作的原因列表：

| 检查项 | 失败信息示例 |
|---|---|
| 兼容源存在 | "无 5.6 对应分支（可用：main/4.26, ue5.7, ue5.8）——需要移植或换引擎" |
| VS 工具链 | "未检测到 VS2022 C++ 工具链，源码构建不可用（仅可装 Release 预编译包）" |
| 引擎完整 | "E:\...\UE_5.6 缺少 Build\BatchFiles\RunUAT.bat" |
| 文件锁 | "TrueGlow.dll 被 UnrealEditor.exe 占用——请先关闭 UE 5.7" |
| 磁盘空间 | 缓存/构建/安装三段所需空间预估 |

安装按钮禁用带原因 tooltip；诊断详情用 InfoBar + 明细面板。部分阻塞（如文件锁）
提供"稍后自动重试"。

## 4. 数据流（安装为例）

```
用户添加源 → sources 拉取/引用 → detect 识别类型与引擎 → 用户勾选
→ install（Release 下载 或 RunUAT 构建，日志流式）→ 落位 → registry 登记
→ 列表刷新（状态点变绿）
```

## 4.1 分类与搜索（列表页交互）

- **维度筛选（chip 组，可叠加）**：
  - 宿主：全部 / UE / Houdini
  - 状态：已安装 / 可更新 / 未安装 / 安装失败
  - 引擎：全部 / 5.4 / 5.7 / 5.8 / …（按 detect 到的引擎动态生成；选中 5.7 时
    列表只显示与 5.7 有关的插件，兼容徽标简化为单态）
  - 来源：GitHub / 本地
- **搜索**：顶部搜索框，模糊匹配名称/描述/仓库名，前缀语法 `engine:5.7`、
  `status:installed`、`type:houdini` 直接进筛选。
- 空结果给"清除筛选"快捷入口；筛选状态记入本地配置（重启保持）。
- UI 参考：winui3 CommandBar（工具条）+ beautifului filter-table（筛选行）+
  AutoSuggestBox（搜索框）；引擎徽标复用 tool-chips 形态。

## 5. 错误处理

| 场景 | 行为 |
|---|---|
| DLL 被编辑器锁定 | 预检测 + 明确提示"关闭 UE 5.7 后重试"，不产生半删状态 |
| 缺 VS 工具链 / 引擎残缺 | 安装前预检直接报缺什么 |
| Release zip 结构异常 | 自动探测根/一层嵌套；再失败报错并保留 zip 供人工检查 |
| git/gh 网络失败 | 本地源不受影响；远端操作提供重试 |
| RunUAT 编译失败 | 日志窗口高亮 error 行，registry 不写入安装记录 |
| registry 损坏 | 启动时 JSON 解析失败 → 备份坏文件后重建空表（引擎重扫描可恢复大部分信息） |

## 6. UI 设计规范（提取自 obs-float-bar）

- 窗口：无边框透明 + 自绘 header（`data-tauri-drag-region`）；主窗约 960×640。
- 外壳：`rgba(22,22,24,0.88)` + `backdrop-filter: blur(24px)` + 1px `rgba(255,255,255,0.08)` + 圆角 16px + `0 8px 32px rgba(0,0,0,0.35)`。
- 字体：`MiSans → Segoe UI Variable → system-ui`；正文 13px / 辅助 11px / 数字 `tabular-nums`。
- 文本层级：主 92% 白 / 次 55% / 数值 82% 加粗 / 分隔符 18%。
- 语义色：绿 `#30d158`（已装最新）/ 黄 `#ffd60a`（可更新）/ 灰 `#8e8e93`（未装或本地源）/ 红 `#ff453a`（失败，1.2s 呼吸闪烁）/ 蓝 `#0a84ff`（主操作）/ 橙 `#ff9f0a`（进行中）。
- 按钮：36px 圆形透明底；hover 9% 白，active 14% 白 + `scale(0.92)`；列表行操作同手感小号版。
- 列表行：info-strip 风格单行紧凑数据，`·` 分隔，引擎徽标小胶囊。
- 日志面板：`rgba(0,0,0,0.22)` 内嵌、等宽字体、error 行红标。
- Toast：底部弹出，圆角 12px，可带"打开目录"动作。
- 动效：`cubic-bezier(0.16,1,0.3,1)` 面板展开；进行中徽标呼吸。

### 6.1 组件级参考：Beautiful UI（ui-reference/beautifului/）

来源 beautifului.dev（MIT）的 21 个 React 组件源码已提取入库，作为组件交互/动效的移植蓝本（Vue 语义重写，不直接引用）。与 obs-float-bar 视觉基调叠加使用：obs-float-bar 定视觉 token（色彩/毛玻璃/密度），Beautiful UI 定组件形态。重点映射：

- 插件列表主表 ← records-table / filter-table（过滤、徽标、批量选择）
- 批量更新/卸载 ← selection-actions（多选浮出操作条）
- 安装进行中 ← loading-state（pixel-grid）+ task-rows（多引擎安装队列）
- 构建日志 ← code-block + streaming-text（逐行浮现）
- 卸载确认 ← approval-card（列路径确认）
- 主界面导航 ← sidebar-nav + search
- 版本对比 ← diff-table（更新时旧→新版本差异）

### 6.2 控件视觉规范：WinUI 3 / Fluent（ui-reference/winui3/）

从 microsoft-ui-xaml（MIT）按需提取 20 个 themeresources XAML（main @ da997f8）。分工：obs-float-bar 定整体基调（毛玻璃外壳/语义色/密度，冲突时优先），beautifului 定组件交互形态，WinUI 3 补控件级视觉细节——四态（Normal/PointerOver/Pressed/Disabled）、圆角 token（控件 4px/弹层 8px，嵌在 16px 外壳内）、亚克力材质参数（微调 backdrop-filter 的着色/噪声）、侧栏选中态、ContentDialog 遮罩规范。实现时查 VisualState 换算成 CSS 变量，不照搬 XAML。

## 7. 测试

- Rust 单测：LauncherInstalled.dat 解析（用本机真实文件快照做 fixture）、uplugin/zip 结构探测、registry 读写与迁移。
- 集成：对 TrueGlow（git 源、双引擎构建）与一个本地目录源各跑 安装→更新→按引擎卸载 全链路，落位到测试引擎目录断言文件与 registry。
- UI：手动验收清单（状态徽标、锁文件提示、日志流）。

## 8. 里程碑

1. detect + registry + 本地目录源 + UE 手动安装（现成二进制拷贝）+ 列表 UI（含分类/搜索）+ 卸载
2. GitHub 源 + Release 下载安装 + 更新检查 + **compat 版本矩阵（分支解析 + EngineVersion 确认）**
3. RunUAT BuildPlugin 构建流水线（日志流）+ **preflight 诊断（工具链/引擎完整性/文件锁）** + 未验证引擎试编译
4. Houdini 检测 + 安装 + packages json 生成 + 卸载
