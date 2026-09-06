#pragma once

#include "pano/core/i_adapter.h"

#include <QListWidget>
#include <QMainWindow>

namespace pano {

class AdapterRegistry;
class Lifecycle;

// Management window: lists registered adapters, toggles enable/disable, shows
// status, exposes an action to open a monitor window for a component.
class ManagerWindow : public QMainWindow {
    Q_OBJECT
public:
    ManagerWindow(Lifecycle* lifecycle, QList<QString> adapterIds, QWidget* parent = nullptr);

signals:
    void openComponentRequested(const QString& componentId);

private slots:
    void onItemChanged(QListWidgetItem* item);
    void onStatusChanged(const QString& id, AdapterStatus status, const QString& message);
    void onOpenDashboard();

private:
    struct Row {
        QListWidgetItem* item = nullptr;
        bool enabled = false;
        QString id;
    };
    void refreshStatus(const QString& id);

    Lifecycle* lifecycle_ = nullptr;
    QListWidget* list_ = nullptr;
    QMap<QString, Row> rows_;
};

} // namespace pano
