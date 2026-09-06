#pragma once

#include <QColor>
#include <QString>
#include <QWidget>

namespace pano {

// A reusable SVG-like car dashboard gauge drawn with QPainter. Mirrors the Rust
// `Gauge.svelte`: 240° arc, needle with tail, red zone above threshold, neon glow.
class GaugeWidget : public QWidget {
    Q_OBJECT
public:
    struct Config {
        double minValue = 0.0;
        double maxValue = 100.0;
        double threshold = 80.0;   // red zone start
        QString unit = QStringLiteral("%");
        QString title;             // shown above / inside
        QString subLabel;          // e.g. "C:" for disk busiest drive
        QColor accent = QColor(0x22, 0xd3, 0xee);      // cyan
        QColor danger = QColor(0xe7, 0x4c, 0x3c);      // red
        bool compact = false;                          // small-screen mode
    };
    explicit GaugeWidget(QWidget* parent = nullptr);

    void setConfig(const Config& cfg);
    void setValue(double value, const QString& textLabel = {});

    double value() const { return value_; }
    const Config& config() const { return cfg_; }

    QSize sizeHint() const override { return QSize(200, 168); }
    QSize minimumSizeHint() const override { return QSize(96, 84); }

protected:
    void paintEvent(QPaintEvent* ev) override;

private:
    void drawCompact(QPainter& p, const QRectF& r);
    void drawFull(QPainter& p, const QRectF& r);
    QPointF pointOn(const QRectF& r, double angleDeg, double radiusFrac) const;

    Config cfg_;
    double value_ = 0.0;
    QString valueText_;
};

} // namespace pano
