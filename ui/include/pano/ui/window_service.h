#pragma once

#include <QList>
#include <QObject>

namespace pano {

class AdapterRegistry;
class SampleStore;
class Lifecycle;
class ManagerWindow;
class MonitorWindow;

// Owns the top-level windows (manager + component monitors) and persists their
// geometry. Mirrors the Rust `pano-window` + `pano-app` assembly layer.
class WindowService : public QObject {
    Q_OBJECT
public:
    WindowService(AdapterRegistry* registry, SampleStore* store, Lifecycle* lifecycle,
                  QObject* parent = nullptr);
    ~WindowService() override;

    ManagerWindow* manager() const { return manager_; }

    void showManager();
    void openComponent(const QString& componentId);
    void restoreLayout();

private:
    void saveGeometry(QWidget* w, const QString& key);
    void restoreGeometry(QWidget* w, const QString& key);

    AdapterRegistry* registry_ = nullptr;
    SampleStore* store_ = nullptr;
    Lifecycle* lifecycle_ = nullptr;
    ManagerWindow* manager_ = nullptr;
    QList<MonitorWindow*> monitors_;
};

} // namespace pano
