#include "pano/core/sample_store.h"

namespace pano {

SampleStore::SampleStore(int capacity, QObject* parent) : QObject(parent), m_capacity_(capacity) {
    qRegisterMetaType<pano::Sample>("pano::Sample");
}

void SampleStore::setCapacity(int capacity) {
    QMutexLocker lock(&m_mutex_);
    m_capacity_ = qMax(1, capacity);
    for (auto& ring : m_rings_) {
        while (ring.size() > m_capacity_)
            ring.removeFirst();
    }
}

int SampleStore::capacity() const {
    QMutexLocker lock(&m_mutex_);
    return m_capacity_;
}

QVector<Sample>& SampleStore::ringFor(const QString& seriesId) {
    return m_rings_[seriesId];
}

void SampleStore::insert(const Sample& s) {
    Sample copy = s;
    {
        QMutexLocker lock(&m_mutex_);
        auto& ring = ringFor(copy.series);
        if (m_capacity_ <= 0)
            return;
        if (ring.size() >= m_capacity_)
            ring.removeFirst();
        ring.push_back(copy);
    }
    emit sampleAppended(copy);
}

void SampleStore::insert(qint64 tsMs, const QString& series, const SampleValue& value) {
    Sample s;
    s.series = series;
    s.timestampMs = tsMs;
    s.value = value;
    insert(s);
}

QVector<Sample> SampleStore::series(const QString& seriesId) const {
    QMutexLocker lock(&m_mutex_);
    return m_rings_.value(seriesId);
}

QMap<QString, Sample> SampleStore::latest() const {
    QMutexLocker lock(&m_mutex_);
    QMap<QString, Sample> out;
    for (auto it = m_rings_.cbegin(); it != m_rings_.cend(); ++it) {
        if (!it.value().isEmpty())
            out.insert(it.key(), it.value().constLast());
    }
    return out;
}

QVector<QString> SampleStore::seriesIds() const {
    QMutexLocker lock(&m_mutex_);
    return m_rings_.keys();
}

bool SampleStore::has(const QString& seriesId) const {
    QMutexLocker lock(&m_mutex_);
    return m_rings_.contains(seriesId);
}

void SampleStore::clearSeries(const QString& seriesId) {
    {
        QMutexLocker lock(&m_mutex_);
        m_rings_.remove(seriesId);
    }
    emit seriesCleared(seriesId);
}

void SampleStore::clear() {
    QMutexLocker lock(&m_mutex_);
    m_rings_.clear();
}

} // namespace pano
