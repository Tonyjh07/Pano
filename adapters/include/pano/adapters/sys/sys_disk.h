#pragma once

#include "pano/adapters/sys/sys_adapter.h"

#include <QJsonObject>

namespace pano {

// System disk usage across fixed logical drives. Windows implementation.
class SysDiskAdapter : public SysAdapter {
    Q_OBJECT
public:
    explicit SysDiskAdapter(QObject* parent = nullptr) { config_ = util::numericConfig(configSchema()); }

    QString id() const override { return QStringLiteral("sys.disk"); }
    QString typeName() const override { return QStringLiteral("磁盘监控"); }
    QVector<SeriesInfo> series() const override;
    QVector<ConfigField> configSchema() const override;
    int defaultSamplingMs() const override { return 2000; }

    bool start(AdapterContext* ctx) override;
    void stop() override;
    QVector<Sample> poll() override;

private:
    AdapterContext* ctx_ = nullptr;
};

} // namespace pano
