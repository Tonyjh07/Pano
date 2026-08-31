# Pano

> Panorama —— 基于 Rust + Tauri v2 的可插拔监控面板

Pano 以**监控面板**为主要用途，同时可扩展为其他「信息采集 + 展示」场景。
核心设计理念：**适配器（可替换，负责获取信息）→ 核心程序 → UI（可替换，声明所需适配器）**。

## 当前状态

🚀 **M1.2 架构重构完成** —— Tauri v2 迁移 + 窗口服务 + 远程数据源预留：

- workspace 五个 crate（core / adapters / window / ui / app）；
- **pano-window 窗口服务**：组件与窗口分离（R1）——组件只声明 `WindowSpec`，
  窗口由主程序装配的窗口服务统一创建 / 控制（全屏 / 置顶 / 显示器绑定 / 位置 / 可见性，
  R2）；布局持久化（`[window.<id>]` 段，重启恢复）；
- **pano-ui 重写**：Tauri 命令层 + 事件桥接（core → 前端，无轮询）+ Svelte 5 前端
  （多窗口 = 多 WebView 独立渲染进程，根治 M1.1 egui 单事件循环挤占问题）；
- **远程数据源预留（R3）**：core 定义轻量 `HttpClient` 抽象 + `Capability::RemoteSource`；
  pano-adapters 提供 `remote/` 共享基座（HTTP 轮询 / WebSocket 推送模板，feature 开关）；
- 示例适配器 `example.counter` / `example.sine`，一致性测试基座全部通过；
- 托盘常驻：关闭窗口 = 隐藏不退出；托盘菜单打开 / 聚焦窗口、退出应用。

## 快速开始

前置：Rust stable、Node（pnpm）、WebView2（Windows）/ WebKit（macOS/Linux）、
tauri CLI（`cargo install tauri-cli --locked`）。
首次先安装前端依赖：

```bash
cd crates/pano-ui/web && pnpm install && cd ../..
```

> **重要**：Tauri 的 **debug 构建加载 devUrl（http://localhost:1420）**，WebView 需要 Vite dev server 在跑，
> 否则窗口打开后界面报 `ERR_CONNECTION_REFUSED`。两种正确姿势：

**方式 A —— 开发模式（一条命令，自动拉起 Vite，推荐）**：

```bash
cargo tauri dev        # 从仓库根目录运行：before 命令自动 `pnpm --dir pano-ui/web dev`
```

**方式 B —— 独立运行（release 构建，前端资源已嵌入，不依赖 dev server）**：

```bash
cd crates/pano-ui/web && pnpm build && cd ../..
cargo build --release
./target/release/pano-app            # Windows 下为 pano-app.exe；多窗口 + 托盘常驻
cargo run -p pano-app -- --headless  # 无头验证模式（跑 3 秒后退出，debug 即可）
```

> 注：tauri CLI 的 before 命令以 `crates/` 为工作目录执行（`pnpm --dir pano-ui/web …`），
> 请从仓库根目录运行 `cargo tauri dev` / `cargo tauri build`。

GUI 模式下应用常驻系统托盘：关闭任意窗口仅隐藏；托盘菜单「退出」才结束进程。
`--log-file <path>` 可把日志追加写入文件。

配置文件 `pano.toml`（不入库）参考 [`pano.toml.example`](pano.toml.example)；
换适配器 = 改 feature 重新编译，如：

```bash
cargo run -p pano-app --no-default-features --features adapter-example-sine
```

## 关键决策（已确认）

| 决策点 | 结论 |
| --- | --- |
| 适配器加载机制 | trait 注册表 + 编译期特性开关（预留插件化边界） |
| 数据流模型 | 推送 + 环形缓冲 + 事件订阅（适配器采样 → 前端 listen，无轮询） |
| GUI 框架 | Tauri v2（每窗口独立 WebView 渲染进程） |
| 前端栈 | Svelte 5 + Vite + TypeScript + uPlot（pano-ui 内部实现，可替换） |
| 窗口模型 | 组件与窗口分离：`WindowSpec` 声明 + pano-window 窗口服务 + `WindowHandle` API |
| 异步运行时 | tokio |

## 文档索引

- [`AGENTS.md`](AGENTS.md) —— 核心开发与测试规范（代理工作指南，含提交审查 / 合并确认流程）
- [`docs/spec.md`](docs/spec.md) —— 规范设计：workspace 组织、命名、错误、日志、配置、测试、依赖、版本
- [`docs/architecture.md`](docs/architecture.md) —— 架构设计：分层、Adapter trait、数据流、线程模型、窗口服务、远程数据源预留
- [`docs/ui.md`](docs/ui.md) —— 图形界面设计：窗口模型、页面、组件、主题、渲染策略
- [`docs/roadmap.md`](docs/roadmap.md) —— 里程碑与实施计划（M0–M4）

## 目录结构

```
pano/
├── Cargo.toml              # workspace（成员：crates/ 下五个 crate）
├── rust-toolchain.toml     # 固定 stable 工具链
├── pano.toml.example       # 配置模板（pano.toml 为本地配置，不入库）
├── AGENTS.md               # 开发与测试规范
├── crates/
│   ├── pano-core/          # 核心：注册表、生命周期、样本存储、配置、HttpClient 抽象、一致性测试基座
│   ├── pano-adapters/      # 内置适配器（每个适配器一个 feature）+ remote/ 远程数据源基座
│   ├── pano-window/        # 窗口服务：WindowService/WindowHandle/MonitorId/布局持久化（Tauri v2 实现）
│   ├── pano-ui/            # Tauri 命令层 + 事件桥接 + 前端工程（web/，Svelte 5）
│   └── pano-app/           # 二进制入口：组装 core + window + ui（Tauri 壳 / 托盘 / 布局写回）
└── docs/                   # 设计文档
```
