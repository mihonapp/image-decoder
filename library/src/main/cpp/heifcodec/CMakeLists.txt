cmake_minimum_required(VERSION 3.14)
project(heifcodec C CXX ASM)
include(FetchContent)

set(CMAKE_BUILD_TYPE Release)

# ── Output directory ─────────────────────────────────────────────────────────
# Point the default shared-library output to the native-libs tree so that
# any .so built by this project (including heifcodec itself) ends up where
# the Rust linker and Android packaging can find it.
#
# CMAKE_CURRENT_SOURCE_DIR = library/src/main/cpp/heifcodec
# ../../rust/native-libs   = library/src/main/rust/native-libs
set(NATIVE_LIBS_OUT "${CMAKE_CURRENT_SOURCE_DIR}/../../rust/native-libs")
set(CMAKE_LIBRARY_OUTPUT_DIRECTORY "${NATIVE_LIBS_OUT}/${ANDROID_ABI}")
set(CMAKE_RUNTIME_OUTPUT_DIRECTORY "${NATIVE_LIBS_OUT}/${ANDROID_ABI}")

# Share the _deps download cache with the parent project to avoid re-downloading.
get_filename_component(deps "../../_deps" REALPATH BASE_DIR "${CMAKE_BINARY_DIR}")
set(FETCHCONTENT_BASE_DIR ${deps})

# ── libde265 (HEVC / HEIC decoder) ───────────────────────────────────────────
option(BUILD_SHARED_LIBS "" OFF)
option(ENABLE_SDL "" OFF)
if(ANDROID AND NOT ANDROID_ABI STREQUAL "x86" AND NOT ANDROID_ABI STREQUAL "x86_64")
  option(DISABLE_SSE "" ON)
endif()

FetchContent_Declare(libde265
  GIT_REPOSITORY  https://github.com/strukturag/libde265.git
  GIT_TAG         v1.0.9
  BINARY_DIR      libde265-build
  SUBBUILD_DIR    libde265-subbuild
)
FetchContent_MakeAvailable(libde265)

# Export variables that our custom FindLIBDE265.cmake (in cmake/) needs.
# The custom module is used instead of libheif's own FindLIBDE265.cmake to
# avoid find_library() failures when the "library" is a CMake target name.
set(LIBDE265_FOUND     TRUE)
set(LIBDE265_LIBRARIES libde265)
set(LIBDE265_INCLUDE_DIR "${libde265_SOURCE_DIR}")

# ── dav1d (AV1 / AVIF decoder) ───────────────────────────────────────────────
# Reuse the existing dav1d cmake script (meson + generate_dav1d_android_cross_compile.sh).
include(../libheif/dav1d.cmake)

# Provide plural alias that libheif 1.19.x may look for.
set(DAV1D_INCLUDE_DIRS ${DAV1D_INCLUDE_DIR})

# ── libheif 1.21.x ───────────────────────────────────────────────────────────
# v1.21.x matches libheif-sys 5.2.0+1.21.2 (the Rust bindings crate).
FetchContent_Declare(libheif
  GIT_REPOSITORY  https://github.com/strukturag/libheif.git
  GIT_TAG         v1.21.2
  PATCH_COMMAND   echo "" > gdk-pixbuf/CMakeLists.txt
  BINARY_DIR      libheif-build
  SUBBUILD_DIR    libheif-subbuild
)

# Use our custom Find modules (cmake/FindLIBDE265.cmake, cmake/FindDAV1D.cmake)
# so that libheif's find_package() calls pick up the FetchContent-built libs.
list(INSERT CMAKE_MODULE_PATH 0 "${CMAKE_CURRENT_SOURCE_DIR}/cmake")

# Disable plugin loading so all enabled codecs are compiled directly into
# libheif.a (built-in) rather than as separate heif_xxx.so runtime plugins.
# The actual libheif variable is ENABLE_PLUGIN_LOADING, not PLUGIN_LOADING.
set(ENABLE_PLUGIN_LOADING OFF CACHE BOOL "" FORCE)

# Enable the two codecs we care about, using FORCE so the values cannot be
# overridden by libheif's own option() calls when it is fetched below.
set(WITH_LIBDE265        ON  CACHE BOOL "" FORCE)
set(WITH_LIBDE265_PLUGIN OFF CACHE BOOL "" FORCE)
set(WITH_DAV1D           ON  CACHE BOOL "" FORCE)
set(WITH_DAV1D_PLUGIN    OFF CACHE BOOL "" FORCE)

# Disable everything we do not need.
set(WITH_EXAMPLES       OFF CACHE BOOL "" FORCE)
set(WITH_GDK_PIXBUF     OFF CACHE BOOL "" FORCE)
set(WITH_X265           OFF CACHE BOOL "" FORCE)
set(WITH_AOM            OFF CACHE BOOL "" FORCE)
set(WITH_RAV1E          OFF CACHE BOOL "" FORCE)
set(WITH_REDUCED_VISIBILITY OFF CACHE BOOL "" FORCE)
set(BUILD_TESTING       OFF CACHE BOOL "" FORCE)

FetchContent_MakeAvailable(libheif)

# ── heifcodec shared library ──────────────────────────────────────────────────
# Bundle libheif + all statically-built codec deps into a single .so so that:
#  a) Rust links against it dynamically (no need for cargo/embedded cmake builds).
#  b) Android loads it at runtime alongside libimagedecoder.so.
#
# --whole-archive ensures every symbol from the static archives (including those
# not directly referenced by heifcodec itself) is exported in the shared library.
add_library(heifcodec SHARED heifcodec_stub.c)
set_target_properties(heifcodec PROPERTIES LINKER_LANGUAGE CXX)

target_link_libraries(heifcodec
  PRIVATE
    -Wl,--whole-archive
      heif
      ${LIBDE265_LIBRARIES}
      ${DAV1D_LIBRARIES}
    -Wl,--no-whole-archive
    android
    log
)

target_include_directories(heifcodec SYSTEM PRIVATE
  "${libheif_SOURCE_DIR}/libheif"
  "${libheif_BINARY_DIR}/libheif"
)

# ── Copy libheifcodec.so to native-libs/{ABI}/ ───────────────────────────────
# AGP overrides CMAKE_LIBRARY_OUTPUT_DIRECTORY with its own build/intermediates
# path, so we cannot rely on set(CMAKE_LIBRARY_OUTPUT_DIRECTORY ...) to place
# the .so where we need it.  Instead, copy it via POST_BUILD to the fixed path
# that the Rust linker's rustc-link-search (in .cargo/config.toml) expects:
#   library/src/main/rust/native-libs/{ABI}/libheifcodec.so
#
# CMAKE_CURRENT_SOURCE_DIR = library/src/main/cpp/heifcodec
# so ../../rust/native-libs   = library/src/main/rust/native-libs
set(NATIVE_LIBS_OUT "${CMAKE_CURRENT_SOURCE_DIR}/../../rust/native-libs/${ANDROID_ABI}")
file(MAKE_DIRECTORY "${NATIVE_LIBS_OUT}")

add_custom_command(TARGET heifcodec POST_BUILD
  COMMAND ${CMAKE_COMMAND} -E copy_if_different
    $<TARGET_FILE:heifcodec>
    "${NATIVE_LIBS_OUT}/libheifcodec.so"
  COMMENT "Copying libheifcodec.so → native-libs/${ANDROID_ABI}/"
)
