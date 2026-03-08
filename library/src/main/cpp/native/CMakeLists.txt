cmake_minimum_required(VERSION 3.14)
project(native_all C CXX ASM)

# Root CMake project that builds all native libraries for the image-decoder
# Android library module:
#
#   1. libheifcodec.so       – shared bundle of libheif + codecs
#                              (used by both C++ and Rust backends)
#   2. libimagedecoder_cpp.so – C++ image decoder (legacy backend)
#
# The Rust backend (libimagedecoder.so) is built separately by Cargo via the
# rust-android-gradle plugin.

add_subdirectory(
  ${CMAKE_CURRENT_SOURCE_DIR}/../heifcodec
  ${CMAKE_CURRENT_BINARY_DIR}/heifcodec
)

add_subdirectory(
  ${CMAKE_CURRENT_SOURCE_DIR}/../cppdecoder
  ${CMAKE_CURRENT_BINARY_DIR}/cppdecoder
)
