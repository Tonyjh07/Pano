# Changelog

## [0.1.0] — C++/Qt 重构（分支 qt-port）

用 **C++17 + Qt6 Widgets** 重写原 Rust/Tauri 实现，对齐原架构与设计决策：

- **core**（`pano-core`）：`IAdapter` 契约、`AdapterRegistry` 工厂注册表、线程安全环形缓冲 `SampleStore`
  （带 `sampleAppended` 事件订阅）、TOML 子集 `Config`、`Lifecycle`（每适配器 `QTimer` 驱动 `poll()`）；
- **adapters**（`pano-adapters`）：系统监控 `sys.cpu` / `sys.mem` / `sys.disk` / `sys.net`
  （Windows API 实现 + 平台条件编译）+ 示例 `example.counter`；
- **ui**（`pano-ui`）：`ManageWindow`（适配器启停/状态）、`MonitorWindow`（`sys-dashboard` 资源仪表盘）、
  `GaugeWidget`（QPainter 240° 弧形仪表 + 红区 + 指示灯 + 霓虹辉光 + 紧凑小屏模式）、`TimeSeriesWidget`、
  `WindowService`（窗口创建/几何持久化）；
- **app**（`pano-app`）：`QApplication` 装配 + `pano.toml` 配置加载 + `--headless` 冒烟；
- **tests**（Qt Test）：`test_core`（SampleStore/Config/Registry/Lifecycle）、`test_adapters`（示例一致性 + sys 各
  适配器 + gauge 几何）、`test_ui`（GaugeGeometry 纯几何 + GaugeWidget 渲染）；ctest 3 项全绿。
- 已知简化：适配器采样在主线程 QTimer（非后台线程）；`docs/` 为 Rust 时代设计遗产。

---

## [0.1.0] — M1.x / M2 开发期（未发布）


### M2 系统监控适配器（分支 m1.2，开发中）

- **新增 `sys.cpu` / `sys.mem` / `sys.disk` / `sys.net` 系统监控适配器**：共用
  `sysinfo`，按平台条件编译（Windows / Linux / macOS）；自定义字段
  （`per_core` / `swap` / `device` / `interface`）经 `config_schema` 渲染。
- **默认特性切换**：`pano-app` / `pano-adapters` 默认只含系统监控适配器（spec §5）；
  示例 / 远程适配器需显式 feature 启用。
- **一致性测试基座适配多 series**：`run_all` 增加 series 参数（主指标），轮询
  等待首样本（超时 = `max(sampling*8, 3s)`），适配 sysinfo 首次刷新冷启动延迟。
- **sys 适配器采样节奏修复**（诊断记录见 roadmap §M2）：`start` 预热刷新；
  interval 用 `MissedTickBehavior::Skip`（避免补爆）；任务内不用 `block_in_place`
  （避免与 time driver 的运行时关闭竞态 panic）。

### M1.2 架构重构：Tauri 迁移 + 窗口服务 + 远程数据源预留（分支 m1.2）

- **新增 `pano-window` crate**：`WindowService` / `WindowHandle` / `MonitorId` /
  布局持久化（`[window.<id>]` 段），Tauri v2 实现（`WebviewWindow` 封装）；
  窗口控制一律经 `WindowHandle`，命令层与组件不直接操作 Tauri 类型；
  建窗定位决策（`persist::placement_of`）：记忆位置优先，失效显示器回退默认不硬失败。
- **pano-ui 重写为 Tauri 命令层 + Svelte 5 前端**：命令层（适配器启停 / 采样 /
  窗口控制 / 快照查询）、事件桥接（`SampleStore` 订阅 → 前端 `listen`，无轮询）、
  前端（多窗口 SPA：管理窗口页签 + 组件窗口 uPlot 实时曲线，Vitest 10 例）；
  egui 界面整版移除（M1.1 实测性能问题根治，见 roadmap §M1.1）。
- **pano-core 扩展**：`UISpec.components`（`ComponentSpec` / `WindowSpec` 纯数据声明）、
  `Capability::RemoteSource`、`HttpClient` 抽象（`HttpResponse` / `HttpError`，dyn-compatible）、
  `AdapterContext.http` 注入点、`SampleStore` 事件订阅（broadcast）、
  `[window.<id>]` 布局持久化配置段、`CoreError::Adapter` / `InvalidAdapterId`、
  `apply_adapter_config` 失败回滚、统一时钟源 `now()`、adapter id 共享校验。
- **pano-adapters 远程数据源基座（R3 预留）**：`remote/http_poll`（reqwest 实现
  `HttpClient` + 轮询骨架）、`remote/ws_push`（断线重连骨架 + 连接状态映射），
  简易指数退避（1s/2s/4s…封顶 60s）；具体远程适配器归 M2。
- **pano-app 装配**：Tauri Builder（命令注册 / 托盘 / 关闭=隐藏 / 布局持久化
  串行写回 / 事件桥接 / headless 模式保留 / `--log-file`）。
- **修复（dev 流程）**：`cargo tauri dev` 的 cwd 与仓库根不一致导致默认配置
  `pano.toml` 找不到——配置文件路径改为从 cwd 逐级向上解析；README「快速开始」
  区分开发模式（`cargo tauri dev` 一条命令）与独立运行（release 构建嵌入前端
  资源，不依赖 dev server）。

### M1.1 UI 多窗口 + 托盘常驻（交付 `a312b04`，已被 M1.2 Tauri 迁移替代）

- egui 多窗口（管理窗口 + 组件窗口）、托盘常驻、组件级窗口置顶 / 全屏。
- 实测暴露 egui 单事件循环性能问题（详见 roadmap §M1.1），M1.2 迁移 Tauri 根治。

### M1 架构骨架

- workspace 四 crate（core / adapters / ui / app）、适配器注册表与生命周期、
  环形缓冲 SampleStore、配置加载与热生效、示例适配器 `example.counter` /
  `example.sine`、一致性测试基座、egui 管理界面。
