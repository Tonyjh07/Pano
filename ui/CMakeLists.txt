find_package(Qt6 REQUIRED COMPONENTS Widgets Core)

add_library(pano-ui STATIC
  src/dashboard_widget.cpp
  src/gauge_widget.cpp
  src/manager_window.cpp
  src/monitor_window.cpp
  src/time_series_widget.cpp
  src/window_service.cpp
  include/pano/ui/components.h
  include/pano/ui/dashboard_widget.h
  include/pano/ui/gauge_geometry.h
  include/pano/ui/gauge_widget.h
  include/pano/ui/manager_window.h
  include/pano/ui/monitor_window.h
  include/pano/ui/time_series_widget.h
  include/pano/ui/window_service.h
)

target_include_directories(pano-ui PUBLIC ${CMAKE_CURRENT_SOURCE_DIR}/include)
target_link_libraries(pano-ui PUBLIC pano-core Qt6::Widgets)

if(MSVC)
  target_compile_options(pano-ui PUBLIC /utf-8)
endif()
