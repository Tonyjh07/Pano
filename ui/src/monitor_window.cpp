#include "pano/ui/monitor_window.h"

#include "pano/core/i_adapter.h"
#include "pano/core/lifecycle.h"
#include "pano/core/sample_store.h"
#include "pano/ui/components.h"
#include "pano/ui/dashboard_widget.h"

namespace pano {

MonitorWindow::MonitorWindow(SampleStore* store, Lifecycle* lifecycle, QWidget* parent)
    : QMainWindow(parent), store_(store), lifecycle_(lifecycle) {
    setWindowTitle(QStringLiteral("Pano 资源仪表盘"));
    resize(860, 540);

    dashboard_ = new DashboardWidget(store_, this);
    setCentralWidget(dashboard_);

    setBoundToComponent(QStringLiteral("sys-dashboard"));
}

void MonitorWindow::setBoundToComponent(const QString& componentId) {
    if (componentId != QStringLiteral("sys-dashboard"))
        return;
    // Wire per-adapter high_threshold config into the dashboard lamps.
    for (const QString& series : ComponentCatalog::builtin().first().series) {
        // threshold lives on the adapter's serial prefix (e.g. "sys.cpu").
        const QString adapterId = series.section(QLatin1Char('.'), 0, 1);
        if (IAdapter* a = lifecycle_->adapter(adapterId)) {
            const QJsonObject cfg = a->config();
            const double th = cfg.value(QStringLiteral("high_threshold")).toDouble(80.0);
            dashboard_->setThreshold(series, th);
        }
    }
    dashboard_->setThreshold(QStringLiteral("sys.cpu.usage"), 80.0);
}

} // namespace pano
