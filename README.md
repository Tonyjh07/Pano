# Pano

> Panorama —— 基于 **C++ + Qt Widgets** 的可插拔监控面板

Pano 以**监控面板**为主要用途，同时可扩展为其他「信息采集 + 展示」场景。
核心设计理念：**适配器（可替换，负责获取信息）→ 核心程序 → UI（可替换，声明所需适配器）**。

> 说明：本仓库 `qt-port` 分支为用 **C++17 + Qt6 Widgets** 对原 Rust/Tauri 实现的重构（架构与设计决策对齐原
> `docs/`，`docs/` 仍为 Rust 时代设计文档，作为设计遗产参考）。原 Rust 实现保留在 `m2.2` 分支。

## 当前状态

- workspace 骨架：`core / adapters / ui / app` 四层 + `tests`（Qt Test），依赖方向为铁律（ui→core，adapters→core）；
- **pano-core**：`IAdapter` 契约、`AdapterRegistry`（编译期注册表）、线程安全环形缓冲 `SampleStore`（带事件订阅）、
  TOML 子集 `Config`、`Lifecycle`（每适配器一个 QTimer 驱动 `poll()` 采样）；
- **pano-adapters**：系统监控 `sys.cpu` / `sys.mem` / `sys.disk` / `sys.net`（Windows API 实现，平台条件编译）+ 示例
  `example.counter`（一致性测试用）；
- **pano-ui**：Qt Widgets 界面——管理窗口（适配器启停 / 状态）、组件窗口（`sys-dashboard` 资源仪表盘），汽车仪表盘式
  240° 弧形 `GaugeWidget`（QPainter，红区 + 指示灯 + 霓虹辉光 + 紧凑小屏模式）、`TimeSeriesWidget` 实时曲线；
- **pano-app**：`QApplication` 装配（注册表 → SampleStore → Lifecycle → WindowService → 管理/组件窗口），
  `--headless` 冒烟模式，`pano.toml` 可选配置。

## 构建与测试

前置：CMake ≥ 3.21、Qt6（Core/Gui/Widgets/Network/Test）、MSVC 2022（Windows）。Qt 建议通过
`aqtinstall` 安装 `qtbase`（含 QtTest）。

```bash
cmake -S . -B build -G "Visual Studio 17 2022" -A x64 -DCMAKE_PREFIX_PATH=<Qt msvc2022_64>
cmake --build build --config Release
ctest --test-dir build -C Release --output-on-failure
```

## 运行

```bash
build/app/Release/pano-app.exe                                # 图形化：管理窗口 + 资源仪表盘
build/app/Release/pano-app.exe --headless                     # 无头冒烟：启动抽样 3s 后退出（CI 用）
```

运行需把 Qt 的 bin 目录加入 PATH（或部署 Qt DLL）。可选配置文件 `pano.toml`（不入库），参考 `pano.toml.example`：
缺省首次运行自动启用 `sys.cpu/mem/disk/net`，并用默认阈值（`high_threshold=80`）。

## 目录结构

```
pano/
├── CMakeLists.txt          # 顶层：Qt6、四层 + tests、enable_testing
├── pano.toml.example       # 配置模板（pano.toml 为本地配置，不入库）
├── core/                   # 核心契约：IAdapter/Registry/SampleStore/Config/Lifecycle
├── adapters/               # 内置适配器：system/sys_* + example.counter
├── ui/                     # Qt Widgets：管理窗口/组件窗口/Gauge/时间序列/窗口服务
├── app/                    # 二进制入口：装配 + 配置加载 + --headless
├── tests/                  # Qt Test：test_core / test_adapters / test_ui（ctest）
└── docs/                   # 设计遗产（Rust 时代）
```

## 关键决策（对齐原设计）

| 决策点 | 结论 |
| --- | --- |
| 适配器加载机制 | `IAdapter` 工厂注册表 + 编译期注册（预留运行期注册边界） |
| 数据流模型 | 推送 + 环形缓冲 + 事件订阅（`SampleStore::sampleAppended`，无轮询） |
| UI 框架 | Qt Widgets（每组件一窗口，QPainter 绘制仪表） |
| 窗口模型 | 组件与窗口分离：`ComponentSpec` 目录 + `WindowService` 创建/持久化几何 |
| 采样驱动 | `Lifecycle` 每适配器 `QTimer`（PreciseTimer）→ 适配器 `poll()` |

## 铁律

- 适配器永远不访问 UI / 窗口；UI 永远不直接操作适配器，只订阅 `SampleStore`；
- 所有数据流只经 `SampleStore`；适配器经 `AdapterContext` 获得时间戳；
- 中文文案一律 `QStringLiteral`；源文件统一 UTF-8（MSVC 加 `/utf-8`）。
