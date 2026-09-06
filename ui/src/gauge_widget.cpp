#include "pano/ui/gauge_widget.h"

#include "pano/ui/gauge_geometry.h"

#include <QColor>
#include <QFontMetrics>
#include <QPainter>
#include <QPainterPath>
#include <QRadialGradient>

#include <cmath>

namespace pano {

namespace {
constexpr double kSweep = 240.0; // degrees
constexpr double kStart = -120.0; // at min value
} // namespace

GaugeWidget::GaugeWidget(QWidget* parent) : QWidget(parent) {
    setSizePolicy(QSizePolicy::Expanding, QSizePolicy::Expanding);
}

void GaugeWidget::setConfig(const Config& cfg) {
    cfg_ = cfg;
    update();
}

void GaugeWidget::setValue(double value, const QString& textLabel) {
    value_ = value;
    valueText_ = textLabel;
    update();
}

QPointF GaugeWidget::pointOn(const QRectF& r, double angleDeg, double radiusFrac) const {
    const double rad = angleDeg * M_PI / 180.0;
    const double cx = r.center().x();
    const double cy = r.top() + r.height() * 0.52;
    const double radius = qMin(r.width(), r.height()) * radiusFrac;
    return QPointF(cx + radius * std::sin(rad), cy - radius * std::cos(rad));
}

void GaugeWidget::paintEvent(QPaintEvent*) {
    QPainter p(this);
    p.setRenderHint(QPainter::Antialiasing, true);
    const QRectF r = rect().adjusted(2, 2, -2, -2);
    if (cfg_.compact || r.height() < 130)
        drawCompact(p, r);
    else
        drawFull(p, r);
}

void GaugeWidget::drawFull(QPainter& p, const QRectF& r) {
    const qreal cx = r.center().x();
    const qreal cy = r.top() + r.height() * 0.52;

    // neon panel
    QRadialGradient bg(QPointF(cx, cy), qMax(r.width(), r.height()) * 0.7);
    bg.setColorAt(0.0, QColor(0x18, 0x1f, 0x2a));
    bg.setColorAt(1.0, QColor(0x0d, 0x12, 0x1a));
    QPainterPath panel;
    panel.addRoundedRect(r, 18, 18);
    p.fillPath(panel, bg);

    // track
    QPen trackPen(Qt::darkGray, qMax(8.0, r.width() * 0.05), Qt::SolidLine, Qt::RoundCap);
    p.setPen(trackPen);
    p.drawArc(QRectF(r.left() + r.width()*0.12, r.top() + r.height()*0.14,
                     r.width()*0.76, r.width()*0.76), static_cast<int>((90 - (kStart + kSweep)) * 16), static_cast<int>(kSweep * 16));

    // progress fill from min to value
    const double aVal = GaugeGeometry::angleFor(value_, cfg_.minValue, cfg_.maxValue);
    if (aVal > kStart) {
        p.setPen(QPen(cfg_.accent, qMax(6.0, r.width() * 0.04), Qt::SolidLine, Qt::RoundCap));
        p.drawArc(QRectF(r.left() + r.width()*0.12, r.top() + r.height()*0.14,
                         r.width()*0.76, r.width()*0.76), static_cast<int>((90 - (kStart + kSweep)) * 16), static_cast<int>(-(aVal - kStart) * 16));
    }

    // red zone (threshold..max)
    const double aTh = GaugeGeometry::angleFor(cfg_.threshold, cfg_.minValue, cfg_.maxValue);
    if (aTh < kStart + kSweep) {
        p.setPen(QPen(cfg_.danger, qMax(6.0, r.width() * 0.035), Qt::SolidLine, Qt::RoundCap));
        p.drawArc(QRectF(r.left() + r.width()*0.12, r.top() + r.height()*0.14,
                         r.width()*0.76, r.width()*0.76), static_cast<int>((90 - aTh) * 16), static_cast<int>(-(kStart + kSweep - aTh) * 16));
    }

    // tick marks
    p.setPen(QPen(QColor(0x88, 0x92, 0xa0), 1));
    const QFontMetrics fm = p.fontMetrics();
    const double tickRadius = qMin(r.width(), r.height()) * 0.44;
    for (int i = 0; i <= 10; ++i) {
        const double a = kStart + kSweep * (i / 10.0);
        const QPointF in( cx + tickRadius * 0.82 * std::sin(a * M_PI / 180.0), cy - tickRadius * 0.82 * std::cos(a * M_PI / 180.0));
        const QPointF out(cx + tickRadius * std::sin(a * M_PI / 180.0), cy - tickRadius * std::cos(a * M_PI / 180.0));
        p.drawLine(in, out);
        if (i % 2 == 0 && !cfg_.compact) {
            const QPointF txt(cx + tickRadius * 0.62 * std::sin(a * M_PI / 180.0), cy - tickRadius * 0.62 * std::cos(a * M_PI / 180.0) - 2);
            p.drawText(QPointF(txt.x(), txt.y()), QString::number(cfg_.minValue + (cfg_.maxValue - cfg_.minValue) * (i / 10.0), 'f', 0));
        }
    }

    // needle
    const double aRad = aVal * M_PI / 180.0;
    const double needleLen = qMin(r.width(), r.height()) * 0.40;
    const QPointF tip(cx + needleLen * std::sin(aRad), cy - needleLen * std::cos(aRad));
    const QPointF tail(cx - needleLen * 0.22 * std::sin(aRad), cy + needleLen * 0.22 * std::cos(aRad));
    p.setPen(QPen(cfg_.accent.lighter(160), qMax(2.5, r.width() * 0.015), Qt::SolidLine, Qt::RoundCap));
    p.drawLine(tail, tip);
    p.setPen(Qt::NoPen);
    p.setBrush(cfg_.accent);
    p.drawEllipse(QPointF(cx, cy), qMax(4.0, r.width() * 0.025), qMax(4.0, r.width() * 0.025));

    // value + title
    const QString valueText = valueText_.isEmpty() ? QString::number(value_, 'f', 0) : valueText_;
    p.setPen(Qt::white);
    QFont vf = font(); vf.setPointSizeF(qMax(9.0, r.height() * 0.11)); vf.setBold(true); p.setFont(vf);
    p.drawText(QRectF(cx - r.width()*0.4, r.top() + r.height()*0.55, r.width()*0.8, r.height()*0.16), Qt::AlignCenter, valueText + cfg_.unit);

    if (!cfg_.title.isEmpty()) {
        p.setPen(QColor(0xbf, 0xc8, 0xd4));
        QFont tf = font(); tf.setPointSizeF(qMax(7.0, r.height() * 0.07)); p.setFont(tf);
        p.drawText(QRectF(cx - r.width()*0.4, r.top() + r.height()*0.82, r.width()*0.8, r.height()*0.12), Qt::AlignCenter, cfg_.title);
    }
}

void GaugeWidget::drawCompact(QPainter& p, const QRectF& r) {
    const qreal cx = r.center().x();
    const qreal cy = r.top() + r.height() * 0.55;

    QRadialGradient bg(QPointF(cx, cy), qMax(r.width(), r.height()) * 0.7);
    bg.setColorAt(0.0, QColor(0x18, 0x1f, 0x2a));
    bg.setColorAt(1.0, QColor(0x0d, 0x12, 0x1a));
    QPainterPath panel; panel.addRoundedRect(r, 8, 8);
    p.fillPath(panel, bg);

    const double aVal = GaugeGeometry::angleFor(value_, cfg_.minValue, cfg_.maxValue);
    const double aTh = GaugeGeometry::angleFor(cfg_.threshold, cfg_.minValue, cfg_.maxValue);

    const qreal arcRect = qMin(r.width(), r.height()) * 0.92;
    QRectF arcR(cx - arcRect/2, cy - arcRect/2, arcRect, arcRect);
    QPen trackPen(Qt::darkGray, qMax(3.0, r.width()*0.08), Qt::SolidLine, Qt::RoundCap);
    p.setPen(trackPen);
    p.drawArc(arcR, static_cast<int>((90 - (kStart + kSweep)) * 16), static_cast<int>(kSweep * 16));
    if (aTh < kStart + kSweep) {
        p.setPen(QPen(cfg_.danger, qMax(2.5, r.width()*0.07), Qt::SolidLine, Qt::RoundCap));
        p.drawArc(arcR, static_cast<int>((90 - aTh) * 16), static_cast<int>(-(kStart + kSweep - aTh) * 16));
    }

    // indicator lamp
    const bool hot = GaugeGeometry::overThreshold(value_, cfg_.threshold);
    const QPointF lamp(cx + arcR.width()/2 + r.width()*0.0, r.top() + r.height()*0.18);
    p.setPen(Qt::NoPen); p.setBrush(hot ? cfg_.danger : QColor(0x2e, 0xcc, 0x71));
    p.drawEllipse(lamp, qMax(3.0, r.height()*0.08), qMax(3.0, r.height()*0.08));

    // value
    const QString valueText = valueText_.isEmpty() ? QString::number(value_, 'f', 0) : valueText_;
    p.setPen(Qt::white);
    QFont vf = font(); vf.setPointSizeF(qMax(8.0, r.height() * 0.24)); vf.setBold(true); p.setFont(vf);
    p.drawText(QRectF(r.left() + 2, cy - r.height()*0.28, r.width() - 4, r.height()*0.4), Qt::AlignCenter, valueText + cfg_.unit);

    if (!cfg_.title.isEmpty()) {
        p.setPen(QColor(0xbf, 0xc8, 0xd4));
        QFont tf = font(); tf.setPointSizeF(qMax(6.0, r.height() * 0.13)); p.setFont(tf);
        p.drawText(QRectF(r.left() + 2, r.bottom() - r.height()*0.32, r.width() - 4, r.height()*0.26), Qt::AlignCenter, cfg_.title);
    }
}

} // namespace pano
