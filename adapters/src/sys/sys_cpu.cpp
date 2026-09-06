#include "pano/adapters/sys/sys_cpu.h"

#if defined(Q_OS_WIN)
#  define WIN32_LEAN_AND_MEAN
#  include <windows.h>
#endif

#include <QDateTime>

namespace pano {

QVector<SeriesInfo> SysCpuAdapter::series() const {
    return { { QStringLiteral("sys.cpu.usage"), QStringLiteral("CPU 使用率"), SampleValue::Double, QStringLiteral("%") } };
}

QVector<ConfigField> SysCpuAdapter::configSchema() const {
    return { { QStringLiteral("high_threshold"), QStringLiteral("高占用阈值"), true, 0.0, 100.0, 80.0 } };
}

#if defined(Q_OS_WIN)

SysCpuAdapter::Counters SysCpuAdapter::readCounters() {
    Counters c;
    FILETIME idle, kernel, user;
    if (GetSystemTimes(&idle, &kernel, &user)) {
        auto toU64 = [](const FILETIME& ft) -> quint64 {
            return (static_cast<quint64>(ft.dwHighDateTime) << 32) | ft.dwLowDateTime;
        };
        c.idle = toU64(idle);
        c.kernel = toU64(kernel);
        c.user = toU64(user);
        c.valid = true;
    }
    return c;
}

double SysCpuAdapter::usagePercent(quint64 idleDelta, quint64 totalDelta) {
    if (totalDelta == 0)
        return 0.0;
    const double busy = static_cast<double>(totalDelta) - static_cast<double>(idleDelta);
    return qBound(0.0, busy * 100.0 / static_cast<double>(totalDelta), 100.0);
}

bool SysCpuAdapter::start(AdapterContext* ctx) {
    ctx_ = ctx;
    haveLast_ = false;
    last_ = readCounters(); // warm-up baseline
    haveLast_ = last_.valid;
    return true;
}

void SysCpuAdapter::stop() {
    ctx_ = nullptr;
    haveLast_ = false;
}

QVector<Sample> SysCpuAdapter::poll() {
    const Counters cur = readCounters();
    if (!haveLast_ || !cur.valid)
        return {};
    const quint64 idleDelta = cur.idle - last_.idle;
    const quint64 totalDelta = (cur.kernel - last_.kernel) + (cur.user - last_.user);
    last_ = cur;
    haveLast_ = true;

    Sample s;
    s.series = QStringLiteral("sys.cpu.usage");
    s.timestampMs = ctx_ ? ctx_->nowMs() : QDateTime::currentMSecsSinceEpoch();
    s.value = SampleValue::fromNumber(usagePercent(idleDelta, totalDelta));
    return { s };
}

#else // non-Windows

SysCpuAdapter::Counters SysCpuAdapter::readCounters() { return {}; }
bool SysCpuAdapter::start(AdapterContext*) { return true; }
void SysCpuAdapter::stop() {}
QVector<Sample> SysCpuAdapter::poll() { return {}; }

#endif

} // namespace pano
