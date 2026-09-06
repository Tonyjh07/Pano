#include "pano/adapters/register.h"

#include "pano/adapters/example_counter.h"
#include "pano/adapters/sys/sys_cpu.h"
#include "pano/adapters/sys/sys_disk.h"
#include "pano/adapters/sys/sys_mem.h"
#include "pano/adapters/sys/sys_net.h"

namespace pano {

void registerBuiltinAdapters(AdapterRegistry& registry) {
    registry.registerFactory(QStringLiteral("example.counter"), [] { return new ExampleCounterAdapter; });
    registry.registerFactory(QStringLiteral("sys.cpu"), [] { return new SysCpuAdapter; });
    registry.registerFactory(QStringLiteral("sys.mem"), [] { return new SysMemAdapter; });
    registry.registerFactory(QStringLiteral("sys.disk"), [] { return new SysDiskAdapter; });
    registry.registerFactory(QStringLiteral("sys.net"), [] { return new SysNetAdapter; });
}

} // namespace pano
