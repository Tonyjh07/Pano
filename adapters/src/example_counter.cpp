#include "pano/adapters/example_counter.h"

#include "pano/adapters/util.h"

#include <cmath>

namespace pano {

namespace {
constexpr double kPi = 3.14159265358979323846;
}

ExampleCounterAdapter::ExampleCounterAdapter(QObject* parent) : IAdapter(parent) {
    config_ = util::numericConfig(configSchema());
}

QVector<Capability> ExampleCounterAdapter::capabilities() const {
    return { Capability::TimeSeries };
}

QVector<SeriesInfo> ExampleCounterAdapter::series() const {
    return { { QStringLiteral("example.counter.value"), QStringLiteral("计数"), SampleValue::Double, QStringLiteral("个") },
             { QStringLiteral("example.counter.sine"),  QStringLiteral("正弦波"), SampleValue::Double, QStringLiteral("") } };
}

QVector<ConfigField> ExampleCounterAdapter::configSchema() const {
    return { { QStringLiteral("step"), QStringLiteral("步长"), true, 1.0, 1000.0, 1.0 } };
}

void ExampleCounterAdapter::setConfig(const QJsonObject& cfg) {
    config_ = cfg;
}

bool ExampleCounterAdapter::start(AdapterContext* ctx) {
    ctx_ = ctx;
    count_ = 0;
    return true;
}

void ExampleCounterAdapter::stop() {
    ctx_ = nullptr;
}

QVector<Sample> ExampleCounterAdapter::poll() {
    count_ += static_cast<qint64>(config_.value(QStringLiteral("step")).toDouble(1.0));
    const qint64 ts = ctx_ ? ctx_->nowMs() : 0;
    const double phase = 2.0 * kPi * (static_cast<double>(count_) / 50.0);
    return { Sample{QStringLiteral("example.counter.value"), ts, SampleValue::fromNumber(static_cast<double>(count_))},
             Sample{QStringLiteral("example.counter.sine"), ts, SampleValue::fromNumber(std::sin(phase))} };
}

} // namespace pano
