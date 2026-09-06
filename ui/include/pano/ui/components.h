#pragma once

#include <QSize>
#include <QString>
#include <QVector>

namespace pano {

// UI component template (mirrors the Rust `ComponentSpec`): a fixed set of series
// it displays plus a default window. Windows reference a component by id.
struct ComponentSpec {
    QString id;
    QString name;
    QVector<QString> series;   // series the component renders
    QString windowTitle;
    QSize defaultSize;
};

// Built-in component catalog. Mirrors `UISpec.components`.
class ComponentCatalog {
public:
    static QVector<ComponentSpec> builtin() {
        return {
            { QStringLiteral("sys-dashboard"),
              QStringLiteral("资源仪表盘"),
              { QStringLiteral("sys.cpu.usage"),
                QStringLiteral("sys.mem.used_percent"),
                QStringLiteral("sys.disk.used_percent"),
                QStringLiteral("sys.net.utilization") },
              QStringLiteral("资源仪表盘"),
              QSize(860, 540) },
        };
    }
};

} // namespace pano
