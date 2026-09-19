# Beautiful UI 组件参考库

来源：<https://www.beautifului.dev/>（TurboProduct 出品的 AI 界面组件展示站，站方声明 MIT License）
抓取方式：Next.js RSC flight payload 中的 `sources` 映射内嵌了全部组件源码，
`extract.mjs` 从首页 HTML 直接提取（2026-09-20 抓取，站点更新后可重跑）。
运行：`curl -sL https://www.beautifului.dev/ -o home.html && node extract.mjs`

## 内容（21 个 React/TSX 组件，共 ~335KB）

| 组件 | 行数 | 对本项目的可用元素 |
|---|---:|---|
| loading-state | 160 | 安装/构建进行中的 pixel-grid 加载动画 |
| thinking-state | 309 | 可展开的执行轨迹（构建步骤日志折叠） |
| streaming-text | 259 | 流式文本（RunUAT 日志逐行浮现） |
| approval-card | 427 | 确认卡片（卸载前列路径确认） |
| tool-chips | 346 | 紧凑工具调用胶囊（插件类型/来源徽标） |
| task-rows | 281 | 任务状态行（安装队列） |
| chat-composer | 248 | 输入组合框（添加 GitHub URL 输入） |
| prompt-bar | 724 | 命令面板式输入（/ 命令、@ 补全） |
| recommendation-card | 202 | 推荐卡片 |
| context-cards | 128 | 上下文卡片 |
| diff-table | 257 | 版本差异表（更新前后的版本对比） |
| records-table | 1070 | 大数据表（插件列表主参考） |
| filter-table | 149 | 表格过滤（按引擎/类型筛选插件） |
| sidebar-nav | 464 | 侧栏导航（主界面分区导航） |
| search | 123 | 搜索框 |
| flowchart | 546 | 流程图（安装流水线可视化） |
| insight-cards | 550 | 洞察卡片（引擎统计/磁盘占用） |
| code-block | 241 | 代码块（构建日志面板参考） |
| fine-tune-card | 359 | 参数微调卡片 |
| selection-actions | 562 | 多选批量操作（批量卸载/更新） |
| agent-screen | 355 | 全屏执行视图 |

## 使用约定

- 这些是 **React 源码**，本项目前端是 **Vue3**——作为设计/交互参考与移植蓝本，
  不直接 import；落地时按语义移植（动画参数、间距、状态机可直接照搬）。
- 依赖站点内部的 7 个 atoms 未随附（Button / GlideMenu / ValuePill / EntityChip /
  StreamText / Shimmer，见各文件 `@/components/...` import）——移植时用本项目
  等价组件替代，或按 obs-float-bar 设计 token 重写。
- 视觉基调仍以设计文档 §6（obs-float-bar 设计语言）为准，本库提供组件级交互与
  动效细节。
