#include "pano/core/lifecycle.h"

namespace pano {

Lifecycle::Lifecycle(AdapterRegistry* registry, SampleStore* store, QObject* parent)
    : QObject(parent), registry_(registry), store_(store), ctx_(store) {}

Lifecycle::Entry* Lifecycle::entry(const QString& id) {
    auto it = entries_.constFind(id);
    return (it == entries_.constEnd()) ? nullptr : it->data();
}

bool Lifecycle::startAdapter(const QString& id, const QJsonObject& config) {
    if (!registry_ || !registry_->contains(id)) {
        qWarning() << "lifecycle: unknown adapter" << id;
        return false;
    }
    stop(id);

    auto e = QSharedPointer<Entry>::create();
    e->config = config.isEmpty() ? QJsonObject() : config;
    e->adapter = registry_->create(id);
    if (!e->adapter) {
        qWarning() << "lifecycle: factory returned null for" << id;
        return false;
    }
    entries_.insert(id, e);

    if (!e->adapter->start(&ctx_)) {
        e->lastError = QStringLiteral("start() failed");
        setStatus(e.data(), AdapterStatus::Error, e->lastError);
        return false;
    }
    setStatus(e.data(), AdapterStatus::Running, {});
    startTimer(e.data());
    return true;
}

void Lifecycle::startTimer(Entry* e) {
    stopTimer(e);
    e->timer = QSharedPointer<QTimer>::create(this);
    e->timer->setInterval(e->adapter->defaultSamplingMs());
    e->timer->setTimerType(Qt::PreciseTimer);
    QObject::connect(e->timer.data(), &QTimer::timeout, this, [this, e]() { onTick(e); });
    e->timer->start();
}

void Lifecycle::stopTimer(Entry* e) {
    if (e->timer) {
        e->timer->stop();
        e->timer->disconnect();
        e->timer.reset();
    }
}

void Lifecycle::onTick(Entry* e) {
    if (!e->adapter || e->status != AdapterStatus::Running)
        return;
    const QVector<Sample> samples = e->adapter->poll();
    for (const Sample& s : samples) {
        if (s.timestampMs <= 0)
            continue;
        store_->insert(s);
    }
}

bool Lifecycle::setAdapterEnabled(const QString& id, bool enabled) {
    Entry* e = entry(id);
    if (!e)
        return false;
    if (enabled) {
        if (e->status == AdapterStatus::Running)
            return true;
        return startAdapter(id, e->config);
    }
    stop(id);
    setStatus(e, AdapterStatus::Stopped, {});
    return true;
}

bool Lifecycle::setAdapterConfig(const QString& id, const QJsonObject& config) {
    Entry* e = entry(id);
    if (!e)
        return false;
    QJsonObject previous = e->config;
    e->config = config;
    if (!e->adapter) {
        return true; // not yet started; config stored for next start
    }
    // Re-apply config and restart to validate.
    if (!startAdapter(id, config)) {
        e->config = previous;
        return false;
    }
    return true;
}

void Lifecycle::stop(const QString& id) {
    Entry* e = entry(id);
    if (!e)
        return;
    stopTimer(e);
    if (e->adapter)
        e->adapter->stop();
    e->status = AdapterStatus::Stopped;
}

void Lifecycle::stopAll() {
    const auto keys = entries_.keys();
    for (const QString& id : keys) {
        Entry* e = entry(id);
        if (e) {
            stopTimer(e);
            if (e->adapter)
                e->adapter->stop();
            e->status = AdapterStatus::Stopped;
        }
    }
}

AdapterStatus Lifecycle::status(const QString& id) const {
    auto it = entries_.constFind(id);
    return (it == entries_.constEnd()) ? AdapterStatus::Stopped : it->data()->status;
}

QString Lifecycle::lastError(const QString& id) const {
    auto it = entries_.constFind(id);
    return (it == entries_.constEnd()) ? QString() : it->data()->lastError;
}

QMap<QString, AdapterStatus> Lifecycle::statuses() const {
    QMap<QString, AdapterStatus> out;
    for (auto it = entries_.constBegin(); it != entries_.constEnd(); ++it)
        out.insert(it.key(), it.value()->status);
    return out;
}

IAdapter* Lifecycle::adapter(const QString& id) const {
    auto it = entries_.constFind(id);
    return (it == entries_.constEnd()) ? nullptr : it->data()->adapter.get();
}

QList<QString> Lifecycle::ids() const {
    return entries_.keys();
}

void Lifecycle::setStatus(Entry* e, AdapterStatus s, const QString& msg) {
    e->status = s;
    e->lastError = msg;
    // find the id for signal
    for (auto it = entries_.constBegin(); it != entries_.constEnd(); ++it) {
        if (it.value().data() == e) {
            emit adapterStatusChanged(it.key(), s, msg);
            break;
        }
    }
}

} // namespace pano
