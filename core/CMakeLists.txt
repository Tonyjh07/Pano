find_package(Qt6 REQUIRED COMPONENTS Core)

add_library(pano-core STATIC
  src/config.cpp
  src/lifecycle.cpp
  src/sample_store.cpp
  src/types.cpp
  include/pano/core/config.h
  include/pano/core/i_adapter.h
  include/pano/core/lifecycle.h
  include/pano/core/registry.h
  include/pano/core/sample_store.h
  include/pano/core/types.h
)

target_include_directories(pano-core PUBLIC ${CMAKE_CURRENT_SOURCE_DIR}/include)
target_link_libraries(pano-core PUBLIC Qt6::Core)

if(MSVC)
  target_compile_options(pano-core PUBLIC /utf-8)
endif()
