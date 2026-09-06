#include "pano/app/run_app.h"

#include "pano/adapters/register.h"
#include "pano/core/config.h"
#include "pano/core/lifecycle.h"
#include "pano/core/registry.h"
#include "pano/core/sample_store.h"
#include "pano/ui/window_service.h"

#include <QFile>
#include <QJsonObject>
#include <QSet>
#include <QTimer>

namespace pano {

namespace {

QJsonObject sectionAsConfig(const Config& cfg, const QString& section) {
    QJsonObject o;
    if (cfg.sections().contains(section)) {
        const auto vals = cfg.sections().value(section);
        for (auto v = vals.constBegin(); v != vals.constEnd(); ++v) {
            const QVariant& qv = v.value();
            if (qv.metaType().id() == QMetaType::Bool)
                o.insert(v.key(), qv.toBool());
            else if (qv.canConvert<double>() && (qv.metaType().id() == QMetaType::Double ||
                                                 qv.metaType().id() == QMetaType::LongLong))
                o.insert(v.key(), qv.toDouble());
            else
                o.insert(v.key(), qv.toString());
        }
    }
    return o;
}

QString findConfigFile() {
    QFile f(QStringLiteral("pano.toml"));
    if (f.exists())
        return f.fileName();
    return {};
}

} // namespace

int runApp(int argc, char** argv) {
    bool headless = false;
    for (int i = 1; i < argc; ++i) {
        if (QString::fromLocal8Bit(argv[i]) == QStringLiteral("--headless"))
            headless = true;
    }

    QApplication app(argc, argv);
    app.setApplicationName(QStringLiteral("Pano"));
    app.setOrganizationName(QStringLiteral("Pano"));

    AdapterRegistry registry;
    registerBuiltinAdapters(registry);

    SampleStore store(600);
    Lifecycle lifecycle(&registry, &store);

    WindowService windows(&registry, &store, &lifecycle);
    windows.showManager();

    // Config: enable adapters declared in pano.toml (or sys adapters on first run).
    Config cfg;
    const QString path = findConfigFile();
    if (!path.isEmpty()) {
        QFile f(path);
        if (f.open(QIODevice::ReadOnly)) {
            QString err;
            cfg = Config::fromToml(QString::fromUtf8(f.readAll()), &err);
        }
    }

    QSet<QString> enabled;
    foreach (const QString& section, cfg.sections().keys()) {
        if (!section.startsWith(QStringLiteral("adapters.")))
            continue;
        const QString id = section.mid(QStringLiteral("adapters.").size());
        if (cfg.boolean(section, QStringLiteral("enabled"), false))
            enabled.insert(id);
    }

    if (cfg.isEmpty()) {
        enabled = { QStringLiteral("sys.cpu"), QStringLiteral("sys.mem"),
                    QStringLiteral("sys.disk"), QStringLiteral("sys.net") };
    }

    for (const QString& id : enabled) {
        if (registry.contains(id))
            lifecycle.startAdapter(id, sectionAsConfig(cfg, QStringLiteral("adapters.") + id));
    }

    windows.openComponent(QStringLiteral("sys-dashboard"));

    if (headless) {
        const int result = 0;
        QTimer::singleShot(3000, []() { QCoreApplication::exit(0); });
        app.exec();
        return result;
    }

    return app.exec();
}

} // namespace pano
