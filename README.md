# Pano

> Panorama —— 基于 Rust + egui 的可插拔监控面板

Pano 以**监控面板**为主要用途，同时可扩展为其他「信息采集 + 展示」场景。
核心设计理念：**适配器（可替换，负责获取信息）→ 核心程序 → UI（可替换，声明所需适配器）**。

## 当前状态

📐 **设计阶段** —— 仅产出设计文档，尚未编写代码。

## 关键决策（已确认）

| 决策点 | 结论 |
| --- | --- |
| 适配器加载机制 | trait 注册表 + 编译期特性开关（预留插件化边界） |
| 数据流模型 | 推送 + 环形缓冲（适配器采样 → UI 按帧读取） |
| 异步运行时 | tokio |
| 初期范围 | 适配器管理界面 + 示例适配器（验证架构） |

## 文档索引

- [`docs/spec.md`](docs/spec.md) —— 规范设计：workspace 组织、命名、错误、日志、配置、测试、依赖、版本
- [`docs/architecture.md`](docs/architecture.md) —— 架构设计：分层、Adapter trait、数据流、线程模型、可替换性
- [`docs/ui.md`](docs/ui.md) —— 图形界面设计：布局、页面、组件、主题、渲染策略
- [`docs/roadmap.md`](docs/roadmap.md) —— 里程碑与实施计划（M0–M4）

## 目标目录结构（设计阶段占位）

```
pano/
├── Cargo.toml              # workspace
├── rust-toolchain.toml
├── pano.toml               # 运行时配置（设计）
├── crates/
│   ├── pano-core/          # 核心：注册表、生命周期、样本存储、配置
│   ├── pano-adapters/      # 内置适配器（每个适配器一个 feature）
│   ├── pano-ui/            # egui 界面（声明所需适配器能力）
│   └── pano-app/           # 二进制入口：组装 core + ui
└── docs/                   # 设计文档
```
