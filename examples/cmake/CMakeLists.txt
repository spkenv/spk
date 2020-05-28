cmake_minimum_required(VERSION 3.13)
cmake_policy(SET CMP0074 NEW) # recognize <Package>_Root variables

project(SPKCmakeExample VERSION 0.1.0)

add_library(SPKCmakeExample SHARED
    src/example.cpp
)
target_include_directories(SPKCmakeExample
    PUBLIC
        $<INSTALL_INTERFACE:include>
        $<BUILD_INTERFACE:${CMAKE_CURRENT_SOURCE_DIR}/export>
    PRIVATE
        ${CMAKE_SOURCE_DIR}/include
)
target_compile_features(SPKCmakeExample
    PUBLIC
        cxx_std_11
)
set_target_properties(SPKCmakeExample
    PROPERTIES
        CMAKE_POSITION_INDEPENDENT_CODE ON
        VERSION ${SPKCmakeExample_VERSION}
        SOVERSION ${SPKCmakeExample_VERSION_MAJOR}
)

# dependency libraries need to be found and linked
# find_package(VnP3 2 REQUIRED)
# target_link_libraries(SPKCmakeExample VnP3::VnP3)



install(
    TARGETS
        SPKCmakeExample
    EXPORT
        SPKCmakeExample
    INCLUDES
        DESTINATION include
    LIBRARY
        DESTINATION lib
)
install(DIRECTORY "export/"
    DESTINATION "include"
)

include(CMakePackageConfigHelpers)
configure_package_config_file(
    cmake/SPKCmakeExampleConfig.cmake.in
    SPKCmakeExampleConfig.cmake
    INSTALL_DESTINATION
        lib/cmake/SPKCmakeExample
)

write_basic_package_version_file(
    SPKCmakeExampleConfigVersion.cmake
    VERSION ${SPKCmakeExample_VERSION}
    COMPATIBILITY SameMajorVersion
)

install(
    EXPORT SPKCmakeExample
    FILE SPKCmakeExampleTargets.cmake
    NAMESPACE SPKCmakeExample::
    DESTINATION lib/cmake/SPKCmakeExample
)

install(
    FILES
        ${CMAKE_CURRENT_BINARY_DIR}/SPKCmakeExampleConfig.cmake
        ${CMAKE_CURRENT_BINARY_DIR}/SPKCmakeExampleConfigVersion.cmake
    DESTINATION
        lib/cmake/SPKCmakeExample
)
