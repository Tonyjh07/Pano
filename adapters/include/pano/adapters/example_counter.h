#pragma once

#include "pano/core/i_adapter.h"

#include <QJsonObject>

namespace pano {

// Example counting adapter. Always available; used for consistency tests and to
// demonstrate the adapter->core->UI pipeline. Mirrors the Rust `example.counter`.
class ExampleCounterAdapter : public IAdapter {
    Q_OBJECT
public:
    explicit ExampleCounterAdapter(QObject* parent = nullptr);

    QString id() const override { return QStringLiteral("example.counter"); }
    QString typeName() const override { return QStringLiteral("示例计数器"); }
    QVector<Capability> capabilities() const override;
    QVector<SeriesInfo> series() const override;
    QVector<ConfigField> configSchema() const override;
    int defaultSamplingMs() const override { return 500; }

    void setConfig(const QJsonObject& cfg) override;
    QJsonObject config() const override { return config_; }

    bool start(AdapterContext* ctx) override;
    void stop() override;
    QVector<Sample> poll() override;

private:
    QJsonObject config_;
    AdapterContext* ctx_ = nullptr;
    qint64 count_ = 0;
};

} // namespace pano
