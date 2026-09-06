#pragma once

#include "pano/adapters/sys/sys_adapter.h"

#include <QJsonObject>

namespace pano {

// System memory usage. Windows implementation via GlobalMemoryStatusEx.
class SysMemAdapter : public SysAdapter {
    Q_OBJECT
public:
    explicit SysMemAdapter(QObject* parent = nullptr) { config_ = util::numericConfig(configSchema()); }

    QString id() const override { return QStringLiteral("sys.mem"); }
    QString typeName() const override { return QStringLiteral("内存监控"); }
    QVector<SeriesInfo> series() const override;
    QVector<ConfigField> configSchema() const override;
    int defaultSamplingMs() const override { return 1000; }

    bool start(AdapterContext* ctx) override;
    void stop() override;
    QVector<Sample> poll() override;

private:
    AdapterContext* ctx_ = nullptr;
};

} // namespace pano
