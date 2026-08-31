# Changelog

## [0.1.0] — M1.x 开发期（未发布）

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
