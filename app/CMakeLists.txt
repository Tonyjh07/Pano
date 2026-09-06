find_package(Qt6 REQUIRED COMPONENTS Widgets Core)

add_executable(pano-app
  src/main.cpp
  src/run_app.cpp
  include/pano/app/run_app.h
)
target_include_directories(pano-app PRIVATE ${CMAKE_CURRENT_SOURCE_DIR}/include)
target_link_libraries(pano-app PRIVATE pano-ui pano-adapters pano-core Qt6::Widgets)

if(MSVC)
  target_compile_options(pano-app PRIVATE /utf-8)
endif()
