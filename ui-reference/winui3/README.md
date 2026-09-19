# WinUI 3 (Fluent) 组件参考库

来源：<https://github.com/microsoft/microsoft-ui-xaml>（MIT License）
版本：`main` @ `da997f8a2314b538a766dc4e66afce2646781a66`（2026-09-20 经 gh API 按需下载，
未整仓克隆——该仓库数百 MB，只取对本项目有用的 20 个样式文件）

## 定位

与 `../beautifului/`（组件交互/动效蓝本）分工：本库提供 **Fluent 设计 token 与控件视觉规范**。
我们是 Vue 前端，这些 XAML 不直接运行——作为配色、圆角、状态（hover/pressed/disabled/
focus）、交互细节的权威参考。视觉基调仍以设计文档 §6（obs-float-bar）为准，冲突时
obs-float-bar 优先，本库补细节。

## 文件清单与用途映射

### 设计 token（4 个）

| 文件 | 内容 | 对本项目 |
|---|---|---|
| `Common_themeresources.xaml` | 系统色/强调色画刷、输入框边框与内边距基准（`10,5,6,6`） | 全局 CSS 变量蓝本 |
| `Common_themeresources_any.xaml` | 55KB 跨主题完整资源：文本色阶（Primary/Secondary/Tertiary）、填充色阶、控件状态画刷 | 深浅色状态色对照表（我们以深色为主） |
| `CornerRadius_themeresources.xaml` | 圆角 token：控件 4px / 弹层 8px（Overlay） | 对照 obs-float-bar 的 16px 外壳，内部小控件取 4-8px |
| `AcrylicBrush_themeresources.xaml` | 亚克力材质参数（着色/噪声/饱和度/模糊半径四档） | 毛玻璃微调参数源（对应 backdrop-filter 的 tint/noise 细节） |

### 控件样式（16 个）

| 文件 | 对应本项目组件 |
|---|---|
| `NavigationView_themeresources.xaml` | 主界面侧栏导航（选中态、展开/收起动效） |
| `ListViewItem_themeresources.xaml` | 插件列表行（hover/选中/拖拽态、圆角行） |
| `ItemContainer_themeresources.xaml` | 列表行通用状态机（新 WinUI 推荐方式） |
| `AutoSuggestBox_themeresources.xaml` | 搜索框（插件过滤） |
| `Button_themeresources.xaml` | 按钮三变体（Standard/Accent/Subtle）+ 全状态 |
| `ToggleButton/ToggleSwitch` | 启用开关（插件启用/禁用） |
| `ContentDialog_themeresources.xaml` | 卸载确认对话框（遮罩/按钮排布） |
| `InfoBar_themeresources.xaml` | 信息/错误横条（严重度四色） |
| `Expander_themeresources.xaml` | 可折叠面板（构建日志、引擎分组） |
| `ProgressBar/ProgressRing` | 安装/构建进度（确定/不确定两态） |
| `CommandBar_themeresources.xaml` | 列表上方工具条（刷新/过滤/批量操作） |
| `MenuFlyout_themeresources.xaml` | 右键上下文菜单 |
| `ToolTip_themeresources.xaml` | 悬浮提示 |
| `ScrollBar_themeresources.xaml` | 滚动条（细 Overlay 风格） |

## 取用方式

实现 Vue 组件时：查对应 XAML 的 VisualState（Normal/PointerOver/Pressed/Disabled）与
资源引用（`ThemeResource` 层级），换算成 CSS 变量与 transition；不照抄 XAML 结构。
