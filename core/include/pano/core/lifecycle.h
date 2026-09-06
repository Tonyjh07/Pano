#pragma once

#include "pano/core/i_adapter.h"
#include "pano/core/registry.h"
#include "pano/core/sample_store.h"

#include <QDateTime>
#include <QMap>
#include <QObject>
#include <QSharedPointer>
#include <QTimer>

namespace pano {

// AdapterContext implementation that routes push-style emission into the store.
class LifecycleContext : public AdapterContext {
public:
    explicit LifecycleContext(SampleStore* store) : store_(store) {}
    void emitValue(const QString& series, const SampleValue& value, qint64 tsMs) override {
        store_->insert(tsMs, series, value);
    }
    qint64 nowMs() const override { return QDateTime::currentMSecsSinceEpoch(); }

private:
    SampleStore* store_;
};

// Drives adapter lifecycle on a per-adapter timer and pushes samples to the
// store. Mirrors the Rust core `Lifecycle` (start/stop/enable/config with the
// adapter running in a background/async context).
class Lifecycle : public QObject {
    Q_OBJECT
public:
    Lifecycle(AdapterRegistry* registry, SampleStore* store, QObject* parent = nullptr);

    // Instantiate + start an adapter. Returns false and sets lastError on failure.
    bool startAdapter(const QString& id, const QJsonObject& config = QJsonObject());

    bool setAdapterEnabled(const QString& id, bool enabled);
    // Apply new config (validates by re-start). Rollback to previous config on failure.
    bool setAdapterConfig(const QString& id, const QJsonObject& config);
    void stop(const QString& id);
    void stopAll();

    AdapterStatus status(const QString& id) const;
    QString lastError(const QString& id) const;
    QMap<QString, AdapterStatus> statuses() const;
    IAdapter* adapter(const QString& id) const;
    QList<QString> ids() const;

signals:
    void adapterStatusChanged(const QString& id, pano::AdapterStatus status, const QString& message);

private:
    struct Entry {
        std::unique_ptr<IAdapter> adapter;
        QJsonObject config;
        QSharedPointer<QTimer> timer;
        AdapterStatus status = AdapterStatus::Stopped;
        QString lastError;
    };

    Entry* entry(const QString& id);
    void startTimer(Entry* e);
    void stopTimer(Entry* e);
    void onTick(Entry* e);
    void setStatus(Entry* e, AdapterStatus s, const QString& msg);

    AdapterRegistry* registry_;
    SampleStore* store_;
    QMap<QString, QSharedPointer<Entry>> entries_;
    LifecycleContext ctx_;
};

} // namespace pano
