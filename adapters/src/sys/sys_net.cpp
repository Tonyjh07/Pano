#include "pano/adapters/sys/sys_net.h"

#if defined(Q_OS_WIN)
#  define WIN32_LEAN_AND_MEAN
#  include <windows.h>
#  include <iphlpapi.h>
#endif  // IF_TYPE_SOFTWARE_LOOPBACK is not exposed via iphlpapi.h on some SDKs

#include <QDateTime>

namespace pano {

namespace {
constexpr DWORD kLoopbackType = 24; // IF_TYPE_SOFTWARE_LOOPBACK
}

QVector<SeriesInfo> SysNetAdapter::series() const {
    return { { QStringLiteral("sys.net.rate_bps"),    QStringLiteral("总速率"),   SampleValue::Double, QStringLiteral("bit/s") },
             { QStringLiteral("sys.net.recv_bps"),    QStringLiteral("接收速率"), SampleValue::Double, QStringLiteral("bit/s") },
             { QStringLiteral("sys.net.sent_bps"),    QStringLiteral("发送速率"), SampleValue::Double, QStringLiteral("bit/s") },
             { QStringLiteral("sys.net.utilization"), QStringLiteral("链路利用率"), SampleValue::Double, QStringLiteral("%") } };
}

QVector<ConfigField> SysNetAdapter::configSchema() const {
    return { { QStringLiteral("link_mbps"), QStringLiteral("参考带宽(Mbps)"), true, 1.0, 100000.0, 1000.0 },
             { QStringLiteral("interface"), QStringLiteral("接口过滤"), false, 0.0, 0.0, 0.0 },
             { QStringLiteral("high_threshold"), QStringLiteral("高占用阈值"), true, 0.0, 100.0, 80.0 } };
}

#if defined(Q_OS_WIN)

SysNetAdapter::IfCounters SysNetAdapter::readCounters() {
    IfCounters c;
    c.valid = false;
    ULONG size = 0;
    if (GetIfTable(nullptr, &size, FALSE) != ERROR_INSUFFICIENT_BUFFER)
        return c;
    QByteArray buf(size, 0);
    PMIB_IFTABLE table = reinterpret_cast<PMIB_IFTABLE>(buf.data());
    if (GetIfTable(table, &size, FALSE) != NO_ERROR)
        return c;

    for (DWORD i = 0; i < table->dwNumEntries; ++i) {
        const MIB_IFROW& row = table->table[i];
        if (row.dwType == kLoopbackType)
            continue;
        c.inBytes += row.dwInOctets;
        c.outBytes += row.dwOutOctets;
    }
    c.valid = true;
    return c;
}

bool SysNetAdapter::start(AdapterContext* ctx) {
    ctx_ = ctx;
    last_ = readCounters();
    haveLast_ = last_.valid;
    lastTs_ = 0;
    return true;
}

void SysNetAdapter::stop() {
    ctx_ = nullptr;
    haveLast_ = false;
}

QVector<Sample> SysNetAdapter::poll() {
    const IfCounters cur = readCounters();
    if (!haveLast_ || !cur.valid)
        return {};
    const qint64 now = ctx_ ? ctx_->nowMs() : QDateTime::currentMSecsSinceEpoch();
    const qint64 prevMs = lastTs_ ? lastTs_ : now;
    const double dtMs = qMax<qint64>(1, now - prevMs);
    const double dtSec = dtMs / 1000.0;

    qint64 inDelta = 0, outDelta = 0;
    if (cur.inBytes >= last_.inBytes) inDelta = static_cast<qint64>(cur.inBytes - last_.inBytes);
    if (cur.outBytes >= last_.outBytes) outDelta = static_cast<qint64>(cur.outBytes - last_.outBytes);

    const double inBps = static_cast<double>(inDelta) / dtSec;
    const double outBps = static_cast<double>(outDelta) / dtSec;
    const double totalBps = inBps + outBps;
    const double refBps = config_.value(QStringLiteral("link_mbps")).toDouble(1000.0) * 125000.0; // 1 Mbps = 125_000 B/s
    const double util = refBps > 0 ? qBound(0.0, totalBps * 100.0 / refBps, 100.0) : 0.0;

    last_ = cur;
    lastTs_ = now;

    return { Sample{QStringLiteral("sys.net.rate_bps"), now, SampleValue::fromNumber(totalBps * 8.0)},
             Sample{QStringLiteral("sys.net.recv_bps"), now, SampleValue::fromNumber(inBps * 8.0)},
             Sample{QStringLiteral("sys.net.sent_bps"), now, SampleValue::fromNumber(outBps * 8.0)},
             Sample{QStringLiteral("sys.net.utilization"), now, SampleValue::fromNumber(util)} };
}

#else

SysNetAdapter::IfCounters SysNetAdapter::readCounters() { return {}; }
bool SysNetAdapter::start(AdapterContext*) { return true; }
void SysNetAdapter::stop() {}
QVector<Sample> SysNetAdapter::poll() { return {}; }

#endif

} // namespace pano
