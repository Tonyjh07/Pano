#pragma once

#include "pano/core/types.h"

#include <QVector>
#include <QWidget>

namespace pano {

// A compact real-time line chart drawn with QPainter. Mirrors the `TimeSeriesPanel`
// widget: shows the recent history of a single series with min/max labels.
class TimeSeriesWidget : public QWidget {
    Q_OBJECT
public:
    explicit TimeSeriesWidget(QWidget* parent = nullptr);

    void setSeries(QVector<Sample> samples, const QString& seriesId = {}, const QString& unit = {});
    void setSeriesId(const QString& id) { seriesId_ = id; }
    void setUnit(const QString& u) { unit_ = u; }

    QSize sizeHint() const override { return QSize(320, 120); }
    QSize minimumSizeHint() const override { return QSize(120, 60); }

protected:
    void paintEvent(QPaintEvent* ev) override;

private:
    QVector<Sample> samples_;
    QString seriesId_;
    QString unit_;
};

} // namespace pano
