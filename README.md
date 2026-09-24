# dcc-plugin-manager

UE / Houdini / Obsidian 插件管理安装器（Tauri 2）：一键安装、版本兼容矩阵、更新、按引擎卸载、安装前诊断。

- [设计文档](docs/specs/2026-09-20-dcc-plugin-manager-design.md)（架构 / 流水线 / UI 规范 · 三套 UI 参考源）
- [实现计划](docs/plans/2026-09-20-implementation-plan.md)（四个里程碑 / 任务级路线图）
- `ui-reference/`：beautifului.dev 全量 21 组件 + WinUI3 Fluent 20 样式（字节级校验入库）

状态：v0.1.2 —— 里程碑 1-6 全部落地（三栖管理 + 设置页 + 打包）；更新闭环完成（安装后
commit/版本/本地摘要基线自动同步，更新对话框预选已装引擎，黄点装完即消）。
