//
// C++ benchmark wrapper — exposes the original C++ decoder through renamed JNI
// entry points so it can coexist with the Rust libimagedecoder.so in the same
// process.  All symbols go through tachiyomi.decoder.CppImageDecoder instead of
// tachiyomi.decoder.ImageDecoder.
//

#include "borders.h"
#include "decoders.h"
#include "java_stream.h"
#include "row_convert.h"
#include <android/bitmap.h>
#include <include/lcms2.h>
#include <jni.h>
#include <vector>

// ---------------------------------------------------------------------------
// Cached class/method IDs for CppImageDecoder
// ---------------------------------------------------------------------------
static jclass cppDecoderCls;
static jmethodID cppDecoderCtor;
static jclass imageTypeCls;
static jmethodID imageTypeCtor;
static jmethodID cppCreateBitmapMethod;

static void init_cpp_benchmark_objects(JNIEnv* env) {
  jclass tmpCls;

  tmpCls = env->FindClass("tachiyomi/decoder/CppImageDecoder");
  cppDecoderCls = (jclass)env->NewGlobalRef(tmpCls);
  cppDecoderCtor = env->GetMethodID(cppDecoderCls, "<init>", "(JII)V");
  cppCreateBitmapMethod = env->GetStaticMethodID(
      cppDecoderCls, "createBitmap", "(II)Landroid/graphics/Bitmap;");

  tmpCls = env->FindClass("tachiyomi/decoder/ImageType");
  imageTypeCls = (jclass)env->NewGlobalRef(tmpCls);
  imageTypeCtor = env->GetMethodID(imageTypeCls, "<init>", "(IZ)V");

  env->DeleteLocalRef(tmpCls);
}

static jobject cpp_create_image_type(JNIEnv* env, jint format,
                                     jboolean isAnimated) {
  return env->NewObject(imageTypeCls, imageTypeCtor, format, isAnimated);
}

// ---------------------------------------------------------------------------
// JNI_OnLoad
// ---------------------------------------------------------------------------
jint JNI_OnLoad(JavaVM* vm, void*) {
  JNIEnv* env;
  if (vm->GetEnv((void**)&env, JNI_VERSION_1_6) == JNI_OK) {
    init_java_stream(env);
    init_cpp_benchmark_objects(env);
  } else {
    return JNI_ERR;
  }
  return JNI_VERSION_1_6;
}

// ---------------------------------------------------------------------------
// CppImageDecoder JNI exports
// ---------------------------------------------------------------------------

extern "C" JNIEXPORT jobject JNICALL
Java_tachiyomi_decoder_CppImageDecoder_nativeNewInstance(
    JNIEnv* env, jclass, jobject jstream, jboolean cropBorders,
    jbyteArray icm_stream) {
  auto stream = read_all_java_stream(env, jstream);
  if (!stream) {
    return nullptr;
  }

  cmsHPROFILE targetProfile = nullptr;
  if (icm_stream) {
    int icm_stream_len = env->GetArrayLength(icm_stream);
    if (icm_stream_len > 0) {
      std::vector<uint8_t> icm_buf(icm_stream_len);
      env->GetByteArrayRegion(icm_stream, 0, icm_stream_len,
                              reinterpret_cast<jbyte*>(icm_buf.data()));
      targetProfile = cmsOpenProfileFromMem(icm_buf.data(), icm_buf.size());
    }
  }

  if (!targetProfile) {
    targetProfile = cmsCreate_sRGBProfile();
  }

  BaseDecoder* decoder;
  try {
    if (false) {
    }
#ifdef HAVE_LIBJPEG
    else if (is_jpeg(stream->bytes)) {
      decoder = new JpegDecoder(std::move(stream), cropBorders, targetProfile);
    }
#endif
#ifdef HAVE_LIBPNG
    else if (is_png(stream->bytes)) {
      decoder = new PngDecoder(std::move(stream), cropBorders, targetProfile);
    }
#endif
#ifdef HAVE_LIBWEBP
    else if (is_webp(stream->bytes)) {
      decoder = new WebpDecoder(std::move(stream), cropBorders, targetProfile);
    }
#endif
#ifdef HAVE_LIBHEIF
    else if (is_libheif_compatible(stream->bytes, stream->size)) {
      decoder = new HeifDecoder(std::move(stream), cropBorders, targetProfile);
    }
#endif
#ifdef HAVE_LIBJXL
    else if (is_jxl(stream->bytes)) {
      decoder =
          new JpegxlDecoder(std::move(stream), cropBorders, targetProfile);
    }
#endif
    else {
      return nullptr;
    }
  } catch (std::exception& ex) {
    return nullptr;
  }

  Rect bounds = decoder->info.bounds;
  return env->NewObject(cppDecoderCls, cppDecoderCtor, (jlong)decoder,
                        (jint)bounds.width, (jint)bounds.height);
}

extern "C" JNIEXPORT jobject JNICALL
Java_tachiyomi_decoder_CppImageDecoder_nativeDecode(JNIEnv* env, jobject,
                                                    jlong decoderPtr,
                                                    jint sampleSize, jint x,
                                                    jint y, jint width,
                                                    jint height) {
  auto* decoder = (BaseDecoder*)decoderPtr;

  Rect bounds = decoder->info.bounds;
  Rect inRect = {x + bounds.x, y + bounds.y, (uint32_t)width, (uint32_t)height};
  Rect outRect = inRect.downsample(sampleSize);
  if (outRect.width == 0 || outRect.height == 0) {
    return nullptr;
  }

  auto* bitmap =
      env->CallStaticObjectMethod(cppDecoderCls, cppCreateBitmapMethod,
                                  (jint)outRect.width, (jint)outRect.height);
  if (!bitmap) {
    return nullptr;
  }

  uint8_t* pixels;
  AndroidBitmap_lockPixels(env, bitmap, (void**)&pixels);
  if (!pixels) {
    return nullptr;
  }

  try {
    std::vector<uint8_t> out_buffer(outRect.width * outRect.height * 4);
    uint8_t* pout_buffer = out_buffer.data();

    decoder->decode(pout_buffer, outRect, inRect, sampleSize);

    if (decoder->useTransform) {
      cmsDoTransform(decoder->transform, pout_buffer, pixels,
                     outRect.width * outRect.height);
      if (decoder->inType == TYPE_CMYK_8 ||
          decoder->inType == TYPE_CMYK_8_REV ||
          decoder->inType == TYPE_GRAY_8) {
        for (int i = 0; i < (int)(outRect.width * outRect.height); i++) {
          pixels[i * 4 + 3] = 255;
        }
      }
    } else {
      memcpy(pixels, out_buffer.data(), outRect.width * outRect.height * 4);
    }
  } catch (std::exception& ex) {
    AndroidBitmap_unlockPixels(env, bitmap);
    return nullptr;
  }

  AndroidBitmap_unlockPixels(env, bitmap);
  return bitmap;
}

extern "C" JNIEXPORT void JNICALL
Java_tachiyomi_decoder_CppImageDecoder_nativeRecycle(JNIEnv*, jobject,
                                                     jlong decoderPtr) {
  auto* decoder = (BaseDecoder*)decoderPtr;
  delete decoder;
}

extern "C" JNIEXPORT jobject JNICALL
Java_tachiyomi_decoder_CppImageDecoder_nativeFindType(JNIEnv* env, jclass,
                                                      jbyteArray array) {
  uint32_t toRead = 32;
  uint32_t size = env->GetArrayLength(array);

  if (size < toRead) {
    return nullptr;
  }

  auto _bytes = std::make_unique<uint8_t[]>(toRead);
  auto bytes = _bytes.get();
  env->GetByteArrayRegion(array, 0, toRead, (jbyte*)bytes);

  if (is_jpeg(bytes)) {
    return cpp_create_image_type(env, 0, false);
  } else if (is_png(bytes)) {
    return cpp_create_image_type(env, 1, false);
  } else if (is_webp(bytes)) {
    try {
#ifdef HAVE_LIBWEBP
      auto decoder = std::make_unique<WebpDecoder>(
          std::make_shared<Stream>(bytes, size), false, nullptr);
      return cpp_create_image_type(env, 2, decoder->info.isAnimated);
#else
      throw std::runtime_error("WebP decoder not available");
#endif
    } catch (std::exception& ex) {
      return cpp_create_image_type(env, 2, false);
    }
  } else if (is_gif(bytes)) {
    return cpp_create_image_type(env, 3, true);
  } else if (is_jxl(bytes)) {
    return cpp_create_image_type(env, 6, false);
  }

  switch (get_ftyp_image_type(bytes, toRead)) {
  case ftyp_image_type_heif:
    return cpp_create_image_type(env, 4, false);
  case ftyp_image_type_avif:
    return cpp_create_image_type(env, 5, false);
  case ftyp_image_type_no:
    break;
  }

  return nullptr;
}
