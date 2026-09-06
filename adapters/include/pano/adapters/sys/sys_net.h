#pragma once

#include "pano/adapters/sys/sys_adapter.h"

#include <QJsonObject>

namespace pano {

// System network rate (bps) and link utilization. Windows implementation via
// the IP Helper table (GetIfTable). Config `link_mbps` sets the reference
// bandwidth used to compute utilization.
class SysNetAdapter : public SysAdapter {
    Q_OBJECT
public:
    explicit SysNetAdapter(QObject* parent = nullptr) { config_ = util::numericConfig(configSchema()); }

    QString id() const override { return QStringLiteral("sys.net"); }
    QString typeName() const override { return QStringLiteral("网络监控"); }
    QVector<SeriesInfo> series() const override;
    QVector<ConfigField> configSchema() const override;
    int defaultSamplingMs() const override { return 1000; }

    bool start(AdapterContext* ctx) override;
    void stop() override;
    QVector<Sample> poll() override;

private:
    struct IfCounters { quint64 inBytes = 0; quint64 outBytes = 0; bool valid = false; };
    IfCounters readCounters();
    AdapterContext* ctx_ = nullptr;
    IfCounters last_;
    bool haveLast_ = false;
    qint64 lastTs_ = 0;
};

} // namespace pano
