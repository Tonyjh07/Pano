# Pano

> Panorama —— 基于 Rust + egui 的可插拔监控面板

Pano 以**监控面板**为主要用途，同时可扩展为其他「信息采集 + 展示」场景。
核心设计理念：**适配器（可替换，负责获取信息）→ 核心程序 → UI（可替换，声明所需适配器）**。

## 当前状态

🚀 **M1 架构骨架完成** —— 全链路「适配器 → core → UI」已跑通：

- workspace 四个 crate（core / adapters / ui / app）；
- 示例适配器 `example.counter`（递增计数）与 `example.sine`（正弦波），feature 开关可替换；
- egui 界面：仪表盘（数值卡 + 实时曲线）、适配器管理页（启停 / 采样间隔热生效、schema 驱动表单）；
- 适配器一致性测试基座，两个示例适配器全部通过。

## 快速开始

```bash
cargo run -p pano-app                # GUI 模式
cargo run -p pano-app -- --headless  # 无头验证模式（跑 3 秒后退出）
```

配置文件 `pano.toml`（不入库）参考 [`pano.toml.example`](pano.toml.example)；
换适配器 = 改 feature 重新编译，如：

```bash
cargo run -p pano-app --no-default-features --features adapter-example-sine
```

## 关键决策（已确认）

| 决策点 | 结论 |
| --- | --- |
| 适配器加载机制 | trait 注册表 + 编译期特性开关（预留插件化边界） |
| 数据流模型 | 推送 + 环形缓冲（适配器采样 → UI 按帧读取） |
| 异步运行时 | tokio |
| 初期范围 | 适配器管理界面 + 示例适配器（验证架构） |

## 文档索引

- [`AGENTS.md`](AGENTS.md) —— 核心开发与测试规范（代理工作指南，含提交审查 / 合并确认流程）
- [`docs/spec.md`](docs/spec.md) —— 规范设计：workspace 组织、命名、错误、日志、配置、测试、依赖、版本
- [`docs/architecture.md`](docs/architecture.md) —— 架构设计：分层、Adapter trait、数据流、线程模型、可替换性
- [`docs/ui.md`](docs/ui.md) —— 图形界面设计：布局、页面、组件、主题、渲染策略
- [`docs/roadmap.md`](docs/roadmap.md) —— 里程碑与实施计划（M0–M4）

## 目录结构

```
pano/
├── Cargo.toml              # workspace（成员：crates/ 下四个 crate）
├── rust-toolchain.toml     # 固定 stable 工具链
├── pano.toml.example       # 配置模板（pano.toml 为本地配置，不入库）
├── AGENTS.md               # 开发与测试规范
├── crates/
│   ├── pano-core/          # 核心：注册表、生命周期、样本存储、配置、一致性测试基座
│   ├── pano-adapters/      # 内置适配器（每个适配器一个 feature）
│   ├── pano-ui/            # egui 界面（声明 UISpec；仅读 core 公共 API）
│   └── pano-app/           # 二进制入口：组装 core + adapters + ui
└── docs/                   # 设计文档
```
