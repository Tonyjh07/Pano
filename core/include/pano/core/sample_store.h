#pragma once

#include "pano/core/types.h"

#include <QMap>
#include <QObject>
#include <QMutex>
#include <QString>
#include <QVector>

#include <functional>

namespace pano {

// Thread-safe bounded buffer of samples, keyed by series. Mirrors the Rust
// `SampleStore`. Each series keeps up to `capacity` samples, dropping the oldest.
// A `sampleAppended` signal notifies subscribers (UI) without polling.
class SampleStore : public QObject {
    Q_OBJECT
public:
    explicit SampleStore(int capacity = 600, QObject* parent = nullptr);

    void setCapacity(int capacity);
    int capacity() const;

    // Insert one sample. Thread-safe. Emits sampleAppended.
    void insert(const Sample& s);
    void insert(qint64 tsMs, const QString& series, const SampleValue& value);

    // Snapshot copy of the history for a series (oldest -> newest).
    QVector<Sample> series(const QString& seriesId) const;

    // Latest value per series.
    QMap<QString, Sample> latest() const;

    QVector<QString> seriesIds() const;
    bool has(const QString& seriesId) const;
    void clearSeries(const QString& seriesId);
    void clear();

signals:
    void sampleAppended(const pano::Sample& sample);
    void seriesCleared(const QString& seriesId);

private:
    QVector<Sample>& ringFor(const QString& seriesId);

    mutable QMutex m_mutex_;
    QMap<QString, QVector<Sample>> m_rings_;
    int m_capacity_ = 600;
};

} // namespace pano

Q_DECLARE_METATYPE(pano::Sample)
