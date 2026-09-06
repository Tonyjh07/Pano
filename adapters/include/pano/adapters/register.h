#pragma once

#include "pano/core/registry.h"

namespace pano {

// Registers all built-in adapters (sys.cpu / mem / disk / net and example.counter)
// into the registry. Feature selection can be layered on top later.
void registerBuiltinAdapters(AdapterRegistry& registry);

} // namespace pano
