# Custom FindLIBDE265 — libde265 was already built via FetchContent.
# This module is placed ahead in CMAKE_MODULE_PATH so that libheif's
# find_package(LIBDE265) uses it instead of libheif's own FindLIBDE265.cmake,
# which fails because find_library() cannot resolve a CMake target name.

if(TARGET libde265)
  set(LIBDE265_FOUND    TRUE)
  set(LIBDE265_LIBRARY  libde265)
  set(LIBDE265_LIBRARIES libde265)
  set(LIBDE265_INCLUDE_DIR  "${libde265_SOURCE_DIR}")
  set(LIBDE265_INCLUDE_DIRS "${libde265_SOURCE_DIR}" "${libde265_BINARY_DIR}")
  # Provide a version string so libheif's version check passes.
  set(LIBDE265_VERSION "1.0.9")
endif()
