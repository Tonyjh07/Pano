#pragma once

#include "pano/core/types.h"

#include <QJsonObject>
#include <QObject>
#include <QVector>

namespace pano {

// Context handed to an adapter so it can report data without knowing the store.
// The lifecycle drives sampling and pushes poll() results; the context also lets
// adapters emit ad-hoc values (e.g. remote pushes).
class AdapterContext {
public:
    virtual ~AdapterContext() = default;
    virtual void emitValue(const QString& series, const SampleValue& value, qint64 timestampMs) = 0;
    virtual qint64 nowMs() const = 0;
};

// Runtime status of an adapter, mirrored from the Rust `AdapterStatus`.
enum class AdapterStatus { Stopped, Running, Error };

// The adapter plugin contract. Mirrors the Rust `Adapter` trait. Every concrete
// adapter is a stateless-ish object owned (via unique_ptr) by the lifecycle.
class IAdapter : public QObject {
    Q_OBJECT
public:
    using Ptr = std::unique_ptr<IAdapter>;

    explicit IAdapter(QObject* parent = nullptr) : QObject(parent) {}

    // ---- metadata ----
    virtual QString id() const = 0;             // e.g. "sys.cpu"
    virtual QString typeName() const = 0;       // human name, e.g. "CPU 监控"
    virtual QVector<Capability> capabilities() const = 0;
    virtual QVector<SeriesInfo> series() const = 0;
    virtual QVector<ConfigField> configSchema() const = 0;
    virtual int defaultSamplingMs() const = 0;

    // ---- configuration ----
    virtual void setConfig(const QJsonObject& cfg) = 0;
    virtual QJsonObject config() const = 0;

    // ---- lifecycle: initialize resources on start, release on stop ----
    virtual bool start(AdapterContext* ctx) = 0;
    virtual void stop() = 0;

    // Produce the next batch of samples. Called by the lifecycle each tick.
    virtual QVector<Sample> poll() = 0;
};

} // namespace pano
