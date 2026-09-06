#include "pano/adapters/sys/sys_disk.h"

#if defined(Q_OS_WIN)
#  define WIN32_LEAN_AND_MEAN
#  include <windows.h>
#endif

#include <QDateTime>

namespace pano {

QVector<SeriesInfo> SysDiskAdapter::series() const {
    return { { QStringLiteral("sys.disk.used_percent"), QStringLiteral("磁盘使用率"), SampleValue::Double, QStringLiteral("%") },
             { QStringLiteral("sys.disk.free_gb"),       QStringLiteral("空闲空间"),   SampleValue::Double, QStringLiteral("GB") },
             { QStringLiteral("sys.disk.total_gb"),      QStringLiteral("总空间"),     SampleValue::Double, QStringLiteral("GB") } };
}

QVector<ConfigField> SysDiskAdapter::configSchema() const {
    return { { QStringLiteral("device"), QStringLiteral("设备过滤"), false, 0.0, 0.0, 0.0 },
             { QStringLiteral("high_threshold"), QStringLiteral("高占用阈值"), true, 0.0, 100.0, 80.0 } };
}

bool SysDiskAdapter::start(AdapterContext* ctx) {
    ctx_ = ctx;
    return true;
}

void SysDiskAdapter::stop() {
    ctx_ = nullptr;
}

QVector<Sample> SysDiskAdapter::poll() {
#if !defined(Q_OS_WIN)
    return {};
#else
    const QString filter = config_.value(QStringLiteral("device")).toString();

    double total = 0.0, free = 0.0;
    wchar_t buf[512] = {0};
    const DWORD len = GetLogicalDriveStringsW(32768, buf);
    wchar_t* drive = buf;
    while (len > 0 && *drive) {
        const UINT type = GetDriveTypeW(drive);
        const bool physical = (type == DRIVE_FIXED || type == DRIVE_REMOVABLE || type == DRIVE_RAMDISK);
        const QString root = QString::fromWCharArray(drive).left(2);
        if (filter.isEmpty() || root.contains(filter, Qt::CaseInsensitive)) {
            if (physical) {
                ULARGE_INTEGER avail, freespace, totalb;
                if (GetDiskFreeSpaceExW(drive, &avail, &totalb, &freespace)) {
                    total += static_cast<double>(totalb.QuadPart);
                    free += static_cast<double>(freespace.QuadPart);
                }
            }
        }
        drive += wcslen(drive) + 1;
    }

    const double gb = 1024.0 * 1024.0 * 1024.0;
    const double totalGb = total / gb;
    const double freeGb = free / gb;
    const double usedGb = totalGb - freeGb;
    const double pct = (totalGb > 0) ? usedGb * 100.0 / totalGb : 0.0;

    const qint64 ts = ctx_ ? ctx_->nowMs() : QDateTime::currentMSecsSinceEpoch();
    return { Sample{QStringLiteral("sys.disk.used_percent"), ts, SampleValue::fromNumber(qBound(0.0, pct, 100.0))},
             Sample{QStringLiteral("sys.disk.free_gb"), ts, SampleValue::fromNumber(freeGb)},
             Sample{QStringLiteral("sys.disk.total_gb"), ts, SampleValue::fromNumber(totalGb)} };
#endif
}

} // namespace pano
