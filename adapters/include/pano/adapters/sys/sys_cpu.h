#pragma once

#include "pano/adapters/sys/sys_adapter.h"

#include <QJsonObject>

namespace pano {

// System CPU usage (overall, percent). Windows implementation via GetSystemTimes.
class SysCpuAdapter : public SysAdapter {
    Q_OBJECT
public:
    explicit SysCpuAdapter(QObject* parent = nullptr) { config_ = util::numericConfig(configSchema()); }

    QString id() const override { return QStringLiteral("sys.cpu"); }
    QString typeName() const override { return QStringLiteral("CPU 监控"); }
    QVector<SeriesInfo> series() const override;
    QVector<ConfigField> configSchema() const override;
    int defaultSamplingMs() const override { return 1000; }

    bool start(AdapterContext* ctx) override;
    void stop() override;
    QVector<Sample> poll() override;

    // Probability-free helper exposed for tests: compute usage from two counter snapshots.
    static double usagePercent(quint64 idleDelta, quint64 totalDelta);

private:
    struct Counters { quint64 idle = 0; quint64 kernel = 0; quint64 user = 0; bool valid = false; };
    Counters readCounters();
    Counters last_;
    bool haveLast_ = false;
    AdapterContext* ctx_ = nullptr;
};

} // namespace pano
