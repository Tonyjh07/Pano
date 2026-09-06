#include "pano/adapters/sys/sys_mem.h"

#if defined(Q_OS_WIN)
#  define WIN32_LEAN_AND_MEAN
#  include <windows.h>
#endif

#include <QDateTime>

namespace pano {

QVector<SeriesInfo> SysMemAdapter::series() const {
    return { { QStringLiteral("sys.mem.used_percent"), QStringLiteral("内存使用率"), SampleValue::Double, QStringLiteral("%") },
             { QStringLiteral("sys.mem.used_gb"),       QStringLiteral("已用内存"),   SampleValue::Double, QStringLiteral("GB") },
             { QStringLiteral("sys.mem.total_gb"),      QStringLiteral("总内存"),     SampleValue::Double, QStringLiteral("GB") } };
}

QVector<ConfigField> SysMemAdapter::configSchema() const {
    return { { QStringLiteral("high_threshold"), QStringLiteral("高占用阈值"), true, 0.0, 100.0, 80.0 } };
}

bool SysMemAdapter::start(AdapterContext* ctx) {
    ctx_ = ctx;
    return true;
}

void SysMemAdapter::stop() {
    ctx_ = nullptr;
}

QVector<Sample> SysMemAdapter::poll() {
#if !defined(Q_OS_WIN)
    return {};
#else
    MEMORYSTATUSEX ms;
    ms.dwLength = sizeof(ms);
    if (!GlobalMemoryStatusEx(&ms))
        return {};
    const double totalGb = static_cast<double>(ms.ullTotalPhys) / (1024.0 * 1024.0 * 1024.0);
    const double usedGb = static_cast<double>(ms.ullTotalPhys - ms.ullAvailPhys) / (1024.0 * 1024.0 * 1024.0);
    const double pct = (ms.ullTotalPhys > 0) ? usedGb * 100.0 / totalGb : 0.0;
    const qint64 ts = ctx_ ? ctx_->nowMs() : QDateTime::currentMSecsSinceEpoch();
    return { Sample{QStringLiteral("sys.mem.used_percent"), ts, SampleValue::fromNumber(pct)},
             Sample{QStringLiteral("sys.mem.used_gb"),      ts, SampleValue::fromNumber(usedGb)},
             Sample{QStringLiteral("sys.mem.total_gb"),     ts, SampleValue::fromNumber(totalGb)} };
#endif
}

} // namespace pano
