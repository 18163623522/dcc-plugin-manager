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

## 4. 数据流（安装为例）

```
用户添加源 → sources 拉取/引用 → detect 识别类型与引擎 → 用户勾选
→ install（Release 下载 或 RunUAT 构建，日志流式）→ 落位 → registry 登记
→ 列表刷新（状态点变绿）
```

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

## 7. 测试

- Rust 单测：LauncherInstalled.dat 解析（用本机真实文件快照做 fixture）、uplugin/zip 结构探测、registry 读写与迁移。
- 集成：对 TrueGlow（git 源、双引擎构建）与一个本地目录源各跑 安装→更新→按引擎卸载 全链路，落位到测试引擎目录断言文件与 registry。
- UI：手动验收清单（状态徽标、锁文件提示、日志流）。

## 8. 里程碑

1. detect + registry + 本地目录源 + UE 手动安装（现成二进制拷贝）+ 列表 UI + 卸载
2. GitHub 源 + Release 下载安装 + 更新检查
3. RunUAT BuildPlugin 构建流水线（日志流）+ 文件锁/工具链预检
4. Houdini 检测 + 安装 + packages json 生成 + 卸载
