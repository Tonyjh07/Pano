#include "pano/ui/manager_window.h"

#include "pano/core/lifecycle.h"
#include "pano/core/registry.h"

#include <QListWidget>
#include <QToolBar>
#include <QAction>

namespace pano {

namespace {
QString statusText(AdapterStatus s) {
    switch (s) {
    case AdapterStatus::Running: return QStringLiteral("运行中");
    case AdapterStatus::Error: return QStringLiteral("错误");
    case AdapterStatus::Stopped: return QStringLiteral("已停止");
    }
    return {};
}
}

ManagerWindow::ManagerWindow(Lifecycle* lifecycle, QList<QString> adapterIds, QWidget* parent)
    : QMainWindow(parent), lifecycle_(lifecycle) {
    setWindowTitle(QStringLiteral("Pano 管理窗口"));
    resize(560, 420);

    list_ = new QListWidget(this);
    setCentralWidget(list_);

    for (const QString& id : adapterIds) {
        IAdapter* probe = nullptr;
        Row row;
        row.id = id;
        auto* item = new QListWidgetItem(id);
        item->setFlags(item->flags() | Qt::ItemIsUserCheckable);
        item->setCheckState(Qt::Unchecked);
        list_->addItem(item);
        row.item = item;
        rows_.insert(id, row);
        refreshStatus(id); // reuses item to append status
        (void)probe;
    }

    connect(list_, &QListWidget::itemChanged, this, &ManagerWindow::onItemChanged);
    connect(lifecycle_, &Lifecycle::adapterStatusChanged, this, &ManagerWindow::onStatusChanged);

    auto* tb = addToolBar(QStringLiteral("操作"));
    tb->setMovable(false);
    tb->addAction(QStringLiteral("打开资源仪表盘"), this, &ManagerWindow::onOpenDashboard);
}

void ManagerWindow::refreshStatus(const QString& id) {
    auto it = rows_.find(id);
    if (it == rows_.end())
        return;
    const AdapterStatus s = lifecycle_->status(id);
    Row& row = it.value();
    if (row.item) {
        const QString err = lifecycle_->lastError(id);
        QString text = id;
        text += QStringLiteral("  ﹒ ") + statusText(s);
        if (!err.isEmpty())
            text += QStringLiteral(" (") + err + QStringLiteral(")");
        row.item->setText(text);
        row.item->setCheckState(s == AdapterStatus::Running ? Qt::Checked : Qt::Unchecked);
    }
}

void ManagerWindow::onItemChanged(QListWidgetItem* item) {
    const QString id = item->text().section(QLatin1Char(' '), 0, 0);
    auto it = rows_.find(id);
    if (it == rows_.end())
        return;
    Row& row = it.value();
    const bool want = item->checkState() == Qt::Checked;
    if (want != (lifecycle_->status(id) == AdapterStatus::Running))
        lifecycle_->setAdapterEnabled(id, want);
}

void ManagerWindow::onStatusChanged(const QString& id, AdapterStatus, const QString&) {
    refreshStatus(id);
}

void ManagerWindow::onOpenDashboard() {
    emit openComponentRequested(QStringLiteral("sys-dashboard"));
}

} // namespace pano
