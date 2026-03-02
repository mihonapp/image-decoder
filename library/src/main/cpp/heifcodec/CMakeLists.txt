cmake_minimum_required(VERSION 3.14)
project(heifcodec C CXX ASM)
include(FetchContent)

set(CMAKE_BUILD_TYPE Release)

# ── Output directory ─────────────────────────────────────────────────────────
# Output the shared library to a FIXED path so that:
#  - Android packaging (via jniLibs.srcDirs in build.gradle) can find it.
#  - The Rust cross-linker (via .cargo/config.toml) can find it at build time.
set(NATIVE_LIBS_OUT "${CMAKE_CURRENT_SOURCE_DIR}/../../../../rust/native-libs")
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

set(LIBDE265_FOUND 1)
set(LIBDE265_LIBRARIES libde265)
set(LIBDE265_INCLUDE_DIR
  "${libde265_SOURCE_DIR}"
  "${libde265_BINARY_DIR}"
)

# ── dav1d (AV1 / AVIF decoder) ───────────────────────────────────────────────
# Reuse the existing dav1d cmake script (meson + generate_dav1d_android_cross_compile.sh).
include(../libheif/dav1d.cmake)

# ── libheif 1.19.x ───────────────────────────────────────────────────────────
# v1.19.x is the minimum series that satisfies the libheif-rs v2 / libheif-sys v5
# API surface used by core/src/decode/heif.rs (v1_19 feature).
FetchContent_Declare(libheif
  GIT_REPOSITORY  https://github.com/strukturag/libheif.git
  GIT_TAG         v1.19.5
  PATCH_COMMAND   echo "" > gdk-pixbuf/CMakeLists.txt
  BINARY_DIR      libheif-build
  SUBBUILD_DIR    libheif-subbuild
)

# Mirror the option defaults used in the original C++ cmake so codec detection
# falls back to the LIBDE265_FOUND / DAV1D_FOUND variables already set above,
# rather than trying (and failing) to run pkg-config on the build host.
option(WITH_EXAMPLES    "" OFF)
option(WITH_GDK_PIXBUF  "" OFF)
option(WITH_LIBDE265    "" OFF)   # allow cmake variable LIBDE265_FOUND to control it
option(WITH_DAV1D       "" OFF)   # allow cmake variable DAV1D_FOUND to control it
option(WITH_X265        "" OFF)
option(WITH_AOM         "" OFF)
option(WITH_RAV1E       "" OFF)

FetchContent_MakeAvailable(libheif)

# ── heifcodec shared library ──────────────────────────────────────────────────
# Bundle libheif + all statically-built codec deps into a single .so so that:
#  a) Rust links against it dynamically (no need for cargo/embedded cmake builds).
#  b) Android loads it at runtime alongside libimagedecoder.so.
#
# --whole-archive ensures every symbol from the static archives (including those
# not directly referenced by heifcodec itself) is exported in the shared library.
add_library(heifcodec SHARED)
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
