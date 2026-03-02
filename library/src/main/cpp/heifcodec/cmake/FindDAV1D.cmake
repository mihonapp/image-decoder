# Custom FindDAV1D — dav1d was already built via the dav1d.cmake script.
# This module is placed ahead in CMAKE_MODULE_PATH so that libheif's
# find_package(DAV1D) uses it instead of libheif's own FindDAV1D.cmake.

if(DAV1D_FOUND OR DAV1D_LIBRARIES)
  set(DAV1D_FOUND    TRUE)
  set(DAV1D_LIBRARY  "${DAV1D_LIBRARIES}")
  # DAV1D_LIBRARIES and DAV1D_INCLUDE_DIR are already set by dav1d.cmake.
endif()
