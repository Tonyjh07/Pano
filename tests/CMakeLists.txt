find_package(Qt6 REQUIRED COMPONENTS Test Widgets Core)

set(COMMON_INCLUDE_DIRS
  ${CMAKE_SOURCE_DIR}/core/include
  ${CMAKE_SOURCE_DIR}/adapters/include
  ${CMAKE_SOURCE_DIR}/ui/include)

# test_core / test_adapters are non-GUI (use QTEST_GUILESS_MAIN).
foreach(t test_core test_adapters)
  add_executable(${t} ${t}.cpp)
  target_link_libraries(${t} PRIVATE pano-core pano-adapters Qt6::Core Qt6::Test)
  target_include_directories(${t} PRIVATE ${COMMON_INCLUDE_DIRS})
  if(MSVC)
    target_compile_options(${t} PRIVATE /utf-8)
  endif()
  add_test(NAME ${t} COMMAND ${t})
endforeach()

# test_ui renders widgets (uses QTEST_MAIN / QApplication).
add_executable(test_ui test_ui.cpp)
target_link_libraries(test_ui PRIVATE pano-ui pano-core Qt6::Widgets Qt6::Test)
target_include_directories(test_ui PRIVATE ${COMMON_INCLUDE_DIRS})
if(MSVC)
  target_compile_options(test_ui PRIVATE /utf-8)
endif()
add_test(NAME test_ui COMMAND test_ui)

# Run all tests with the Qt offscreen platform so GUI tests (test_ui) can run
# headlessly under ctest without needing the windows platform plugin.
set_tests_properties(test_core test_adapters test_ui PROPERTIES
  ENVIRONMENT "QT_QPA_PLATFORM=offscreen")
