# AGENTS.md — Pano (C++/Qt) 开发规范

本文件是 Pano C++/Qt 重构分支的开发与测试规范，供代理（agent）与协作者使用。

## 项目性质

Rust/Tauri 监控面板的 **C++17 + Qt6 Widgets** 重构。分层：`core → adapters / ui`，`app` 为装配入口。
依赖方向为铁律：`ui` 与 `adapters` 只能依赖 `core`；`app` 依赖全部。

## 构建 / 测试命令

前置：CMake ≥ 3.21、Qt6（msvc2022_64，含 QtTest）、MSVC 2022。

```bash
cmake -S . -B build -G "Visual Studio 17 2022" -A x64 -DCMAKE_PREFIX_PATH=<Qt msvc2022_64>
cmake --build build --config Release
ctest --test-dir build -C Release --output-on-failure     # 3 个测试（test_core/test_adapters/test_ui）
```

运行（需 Qt bin 在 PATH）：
```bash
build/app/Release/pano-app.exe
build/app/Release/pano-app.exe --headless                 # CI 冒烟：抽样 3s 后退出
```

## 测试约定

- 非 GUI 测试用 `QTEST_GUILESS_MAIN`（`test_core` / `test_adapters`）；GUI/widget 测试用 `QTEST_MAIN`（`test_ui`）。
- 测试统一以 `QT_QPA_PLATFORM=offscreen` 运行（`tests/CMakeLists.txt` 的 `set_tests_properties`），保证 ctest 无需窗口平台插件。
- 新增适配器必须提供一致性测试：`start → poll`（等待首样本）→ `stop` 后不再产出；校验 series 值与域。
- `gauge_geometry.*` 为纯几何助手，必须直接单测（不依赖 QApplication）。

## 编码约定

- C++17；MSVC 目标一律加 `/utf-8`（各子目录 CMake 已配置）。
- 中文文案一律 `QStringLiteral(...)`；不要用普通字符串字面量承载中文字符。
- Qt 类（含信号槽）用 `Q_OBJECT`；`CMAKE_AUTOMOC` 已全局开启。
- 错误处理：适配器 `start()` 返回 bool + `Lifecycle` 记录 `lastError`；配置应用失败要回滚旧值。
- 线程安全：`SampleStore` 内部 `QMutex` 保护，供未来异步采样；当前采样在主线程 `QTimer`。
- 新适配器请在 `adapters/src/register.cpp` 的 `registerBuiltinAdapters` 注册，并在 `tests/test_adapters.cpp` 补测。

## 铁律

- 适配器永不访问 UI/窗口；UI 永不直接操作适配器，只订阅 `SampleStore`。
- 数据只经 `SampleStore` 流通；适配器时间戳经 `AdapterContext::nowMs()`。
- 提交前：`ctest` 全绿；不要在有未被 `git status` 确认的中间产物时提交。

## 文档

`docs/` 目录为 Rust 时代设计遗产（架构/UI/里程碑），设计意图仍有参考价值，但其中 Tauri/egui/卡口细节不再适用。
