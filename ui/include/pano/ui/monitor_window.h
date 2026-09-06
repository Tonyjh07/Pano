#pragma once

#include <QMainWindow>

namespace pano {

class SampleStore;
class Lifecycle;
class DashboardWidget;

// A component window: hosts a component widget (the sys-dashboard) and wires the
// adapter thresholds into it. Mirrors the Rust component window (1 window = 1
// component instance).
class MonitorWindow : public QMainWindow {
    Q_OBJECT
public:
    MonitorWindow(SampleStore* store, Lifecycle* lifecycle, QWidget* parent = nullptr);

    void setBoundToComponent(const QString& componentId);

private:
    SampleStore* store_;
    Lifecycle* lifecycle_;
    DashboardWidget* dashboard_ = nullptr;
};

} // namespace pano
