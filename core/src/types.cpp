#include "pano/core/types.h"

namespace pano {

QString capabilityName(Capability c) {
    switch (c) {
    case Capability::TimeSeries: return QStringLiteral("timeseries");
    case Capability::SystemInfo: return QStringLiteral("sysinfo");
    case Capability::RemoteSource: return QStringLiteral("remote");
    }
    return {};
}

} // namespace pano
