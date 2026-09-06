#include "pano/ui/dashboard_widget.h"

#include "pano/core/sample_store.h"
#include "pano/core/types.h"

#include <QGridLayout>
#include <QHBoxLayout>
#include <QVBoxLayout>

namespace pano {

namespace {
constexpr int kMaxHistory = 200;
}

DashboardWidget::DashboardWidget(SampleStore* store, QWidget* parent)
    : QWidget(parent), store_(store) {
    cpuGauge_ = new GaugeWidget(this);
    memGauge_ = new GaugeWidget(this);
    diskGauge_ = new GaugeWidget(this);
    netGauge_ = new GaugeWidget(this);
    cpuChart_ = new TimeSeriesWidget(this);
    netChart_ = new TimeSeriesWidget(this);

    cpuGauge_->setConfig(GaugeWidget::Config{0, 100, 80, "%", "CPU"});
    memGauge_->setConfig(GaugeWidget::Config{0, 100, 80, "%", "内存"});
    diskGauge_->setConfig(GaugeWidget::Config{0, 100, 80, "%", "磁盘"});
    netGauge_->setConfig(GaugeWidget::Config{0, 100, 80, "%", "网络"});

    rebuild();

    if (store_)
        connect(store_, &SampleStore::sampleAppended, this, &DashboardWidget::onSample);
}

void DashboardWidget::setThreshold(const QString& series, double threshold) {
    thresholds_.insert(series, threshold);
}

void DashboardWidget::setCompact(bool compact) {
    compact_ = compact;
    rebuild();
    update();
}

void DashboardWidget::resizeEvent(QResizeEvent*) {
    const bool compact = compact_ || height() < 140;
    if (compact != cpuGauge_->config().compact)
        setCompact(compact);
}

void DashboardWidget::rebuild() {
    if (QLayout* old = layout()) {
        while (QLayoutItem* item = old->takeAt(0)) {
            if (QWidget* w = item->widget()) w->hide();
            delete item;
        }
        delete old;
    }

    if (compact_) {
        auto* h = new QHBoxLayout(this);
        h->setContentsMargins(4, 4, 4, 4);
        h->setSpacing(6);
        for (GaugeWidget* g : {cpuGauge_, memGauge_, diskGauge_, netGauge_}) {
            GaugeWidget::Config c = g->config();
            c.compact = true;
            g->setConfig(c);
            h->addWidget(g, 1);
        }
        cpuChart_->hide();
        netChart_->hide();
    } else {
        auto* v = new QVBoxLayout(this);
        v->setContentsMargins(12, 12, 12, 12);
        v->setSpacing(8);

        auto* grid = new QGridLayout;
        grid->setSpacing(8);
        for (GaugeWidget* g : {cpuGauge_, memGauge_, diskGauge_, netGauge_}) {
            GaugeWidget::Config c = g->config();
            c.compact = false;
            g->setConfig(c);
        }
        grid->addWidget(cpuGauge_, 0, 0);
        grid->addWidget(memGauge_, 0, 1);
        grid->addWidget(diskGauge_, 1, 0);
        grid->addWidget(netGauge_, 1, 1);
        v->addLayout(grid, 3);

        auto* charts = new QGridLayout;
        charts->addWidget(cpuChart_, 0, 0);
        charts->addWidget(netChart_, 0, 1);
        v->addLayout(charts, 1);

        cpuChart_->show();
        netChart_->show();
    }
}

void DashboardWidget::onSample(const Sample& sample) {
    routeSample(sample.series, sample.value.isNumber() ? sample.value.number : 0.0);
    appendHistory(sample.series, sample);
}

void DashboardWidget::routeSample(const QString& series, double value) {
    auto thresholdOf = [&](const QString& s) { return thresholds_.value(s, 80.0); };
    if (series == QStringLiteral("sys.cpu.usage")) {
        cpuGauge_->setValue(value);
        cpuGauge_->setConfig({cpuGauge_->config().minValue, cpuGauge_->config().maxValue,
                              thresholdOf(series), "%", "CPU"});
    } else if (series == QStringLiteral("sys.mem.used_percent")) {
        memGauge_->setValue(value);
        memGauge_->setConfig({0, 100, thresholdOf(series), "%", "内存"});
    } else if (series == QStringLiteral("sys.disk.used_percent")) {
        diskGauge_->setValue(value);
        diskGauge_->setConfig({0, 100, thresholdOf(series), "%", "磁盘"});
    } else if (series == QStringLiteral("sys.net.utilization")) {
        netGauge_->setValue(value);
        netGauge_->setConfig({0, 100, thresholdOf(series), "%", "网络"});
    }

    if (series == QStringLiteral("sys.cpu.usage"))
        cpuChart_->setSeries(history_.value(series), QStringLiteral("sys.cpu.usage"), QStringLiteral("%"));
    else if (series == QStringLiteral("sys.net.utilization"))
        netChart_->setSeries(history_.value(series), QStringLiteral("sys.net.utilization"), QStringLiteral("%"));
}

void DashboardWidget::appendHistory(const QString& series, const Sample& s) {
    QVector<Sample>& h = history_[series];
    h.push_back(s);
    while (h.size() > kMaxHistory)
        h.removeFirst();
}

} // namespace pano
