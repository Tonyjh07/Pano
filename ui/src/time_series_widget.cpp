#include "pano/ui/time_series_widget.h"

#include <QPainter>
#include <QPainterPath>

#include <algorithm>

namespace pano {

TimeSeriesWidget::TimeSeriesWidget(QWidget* parent) : QWidget(parent) {
    setSizePolicy(QSizePolicy::Expanding, QSizePolicy::Expanding);
}

void TimeSeriesWidget::setSeries(QVector<Sample> samples, const QString& seriesId, const QString& unit) {
    if (!seriesId.isEmpty()) seriesId_ = seriesId;
    if (!unit.isEmpty()) unit_ = unit;
    samples_ = std::move(samples);
    update();
}

void TimeSeriesWidget::paintEvent(QPaintEvent*) {
    QPainter p(this);
    p.setRenderHint(QPainter::Antialiasing, true);
    const QRectF r = rect().adjusted(2, 2, -2, -2);
    p.fillRect(r, QColor(0x10, 0x15, 0x1d));
    p.setPen(QPen(QColor(0x2a, 0x33, 0x40), 1));
    p.drawRect(r);

    if (samples_.size() < 2)
        return;

    // collect numeric values
    QVector<double> vals;
    vals.reserve(samples_.size());
    for (const Sample& s : samples_)
        if (s.value.isNumber())
            vals.push_back(s.value.number);
    if (vals.size() < 2)
        return;

    double mn = *std::min_element(vals.constBegin(), vals.constEnd());
    double mx = *std::max_element(vals.constBegin(), vals.constEnd());
    if (mx - mn < 1e-9) { mn -= 1.0; mx += 1.0; }

    const double pad = 6.0;
    const double w = r.width() - 2 * pad;
    const double h = r.height() - 2 * pad;

    auto px = [&](int i) { double t = static_cast<double>(i) / (samples_.size() - 1); return r.left() + pad + t * w; };
    auto py = [&](double v) { double t = (v - mn) / (mx - mn); return r.bottom() - pad - t * h; };

    QPainterPath path;
    path.moveTo(px(0), py(vals[0]));
    for (int i = 1; i < vals.size(); ++i)
        path.lineTo(px(i), py(vals[i]));

    p.setBrush(QColor(0x22, 0xd3, 0xee, 40));
    QPainterPath fill = path;
    fill.lineTo(px(vals.size() - 1), r.bottom() - pad);
    fill.lineTo(px(0), r.bottom() - pad);
    fill.closeSubpath();
    p.fillPath(fill, QColor(0x22, 0xd3, 0xee, 40));

    p.setPen(QPen(QColor(0x22, 0xd3, 0xee), 2));
    p.setBrush(Qt::NoBrush);
    p.drawPath(path);

    // min/max labels
    p.setPen(QColor(0xbf, 0xc8, 0xd4));
    QFont f = font(); f.setPointSizeF(7.5); p.setFont(f);
    p.drawText(QRectF(r.left() + pad, r.top(), w * 0.99, 14), Qt::AlignLeft | Qt::AlignTop, QString::number(mx, 'f', 1));
    p.drawText(QRectF(r.left() + pad, r.bottom() - pad, w * 0.99, 14), Qt::AlignLeft | Qt::AlignBottom, QString::number(mn, 'f', 1));
}

} // namespace pano
