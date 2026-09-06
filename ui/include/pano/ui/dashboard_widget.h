#pragma once

#include "pano/ui/gauge_widget.h"
#include "pano/ui/time_series_widget.h"

#include <QHash>
#include <QMap>
#include <QPointer>
#include <QWidget>

namespace pano {
class SampleStore;
class Sample;

// The `sys-dashboard` composite widget. Mirrors the Rust `SysDashboard.svelte`:
// two big gauges (CPU, memory), two small gauges (disk, network), indicator lamps,
// and live time-series charts. It subscribes to the SampleStore and renders the
// latest values; it never touches adapters directly.
class DashboardWidget : public QWidget {
    Q_OBJECT
public:
    explicit DashboardWidget(SampleStore* store, QWidget* parent = nullptr);

    void setThreshold(const QString& series, double threshold);
    void setCompact(bool compact);
    bool isCompact() const { return compact_; }

protected:
    void resizeEvent(QResizeEvent* ev) override;

private slots:
    void onSample(const pano::Sample& sample);

private:
    void rebuild();
    void routeSample(const QString& series, double value);
    void appendHistory(const QString& series, const Sample& s);

    QPointer<SampleStore> store_;
    QMap<QString, double> thresholds_;
    QHash<QString, QVector<Sample>> history_;

    GaugeWidget* cpuGauge_ = nullptr;
    GaugeWidget* memGauge_ = nullptr;
    GaugeWidget* diskGauge_ = nullptr;
    GaugeWidget* netGauge_ = nullptr;
    TimeSeriesWidget* cpuChart_ = nullptr;
    TimeSeriesWidget* netChart_ = nullptr;
    bool compact_ = false;
};

} // namespace pano
