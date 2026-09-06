find_package(Qt6 REQUIRED COMPONENTS Core)

add_library(pano-adapters STATIC
  src/example_counter.cpp
  src/register.cpp
  src/sys/sys_cpu.cpp
  src/sys/sys_disk.cpp
  src/sys/sys_mem.cpp
  src/sys/sys_net.cpp
  include/pano/adapters/example_counter.h
  include/pano/adapters/register.h
  include/pano/adapters/util.h
  include/pano/adapters/sys/sys_adapter.h
  include/pano/adapters/sys/sys_cpu.h
  include/pano/adapters/sys/sys_disk.h
  include/pano/adapters/sys/sys_mem.h
  include/pano/adapters/sys/sys_net.h
)

target_include_directories(pano-adapters PUBLIC ${CMAKE_CURRENT_SOURCE_DIR}/include)
target_link_libraries(pano-adapters PUBLIC pano-core)

if(WIN32)
  target_link_libraries(pano-adapters PUBLIC iphlpapi)
endif()

if(MSVC)
  target_compile_options(pano-adapters PUBLIC /utf-8)
endif()
