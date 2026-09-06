#include "pano/ui/window_service.h"

#include "pano/core/lifecycle.h"
#include "pano/core/registry.h"
#include "pano/core/sample_store.h"
#include "pano/ui/manager_window.h"
#include "pano/ui/monitor_window.h"

#include <QSettings>
#include <QTimer>

namespace pano {

WindowService::WindowService(AdapterRegistry* registry, SampleStore* store, Lifecycle* lifecycle,
                             QObject* parent)
    : QObject(parent), registry_(registry), store_(store), lifecycle_(lifecycle) {
    manager_ = new ManagerWindow(lifecycle_, registry_->ids(), nullptr);
    connect(manager_, &ManagerWindow::openComponentRequested, this, &WindowService::openComponent);
}

WindowService::~WindowService() {
    for (MonitorWindow* m : monitors_)
        delete m;
    delete manager_;
}

void WindowService::showManager() {
    manager_->show();
    restoreGeometry(manager_, QStringLiteral("window.manager"));
}

void WindowService::openComponent(const QString& componentId) {
    auto* w = new MonitorWindow(store_, lifecycle_, nullptr);
    w->setAttribute(Qt::WA_DeleteOnClose, true);
    monitors_.append(w);
    w->show();
    restoreGeometry(w, QStringLiteral("window.monitor") + QString::number(monitors_.size()));
    connect(w, &QObject::destroyed, this, [this, w]() { monitors_.removeAll(w); });
}

void WindowService::restoreLayout() {
    // Restore sticky monitor defaults; re-open the dashboard on first run.
    for (MonitorWindow* m : qAsConst(monitors_))
        restoreGeometry(m, QStringLiteral("window.monitor"));
}

void WindowService::saveGeometry(QWidget* w, const QString& key) {
    if (!w)
        return;
    QSettings s(QStringLiteral("Pano"), QStringLiteral("Pano"));
    s.setValue(key + QStringLiteral(".geometry"), w->saveGeometry());
}

void WindowService::restoreGeometry(QWidget* w, const QString& key) {
    if (!w)
        return;
    QSettings s(QStringLiteral("Pano"), QStringLiteral("Pano"));
    const QByteArray g = s.value(key + QStringLiteral(".geometry")).toByteArray();
    if (!g.isEmpty())
        w->restoreGeometry(g);
}

} // namespace pano
