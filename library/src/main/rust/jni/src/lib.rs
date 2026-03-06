use imagedecoder_core::decode::{self, Decoder};
use imagedecoder_core::types::Rect;
use jni::objects::{JByteArray, JClass, JObject, JValue};
use jni::sys::{jboolean, jint, jlong, jobject};
use jni::{jni_sig, jni_str, Env, EnvUnowned};
use std::ffi::c_void;

// NDK bitmap functions
extern "C" {
    fn AndroidBitmap_lockPixels(
        env: *mut jni::sys::JNIEnv,
        jbitmap: jobject,
        addrPtr: *mut *mut c_void,
    ) -> i32;
    fn AndroidBitmap_unlockPixels(env: *mut jni::sys::JNIEnv, jbitmap: jobject) -> i32;
}

// ---------------------------------------------------------------------------
// Cached class/method IDs (set once in JNI_OnLoad)
// ---------------------------------------------------------------------------

static mut IMAGE_DECODER_CLS: jni::sys::jclass = std::ptr::null_mut();
static mut IMAGE_DECODER_CTOR: jni::sys::jmethodID = std::ptr::null_mut();
static mut IMAGE_TYPE_CLS: jni::sys::jclass = std::ptr::null_mut();
static mut IMAGE_TYPE_CTOR: jni::sys::jmethodID = std::ptr::null_mut();
static mut CREATE_BITMAP_METHOD: jni::sys::jmethodID = std::ptr::null_mut();

#[unsafe(no_mangle)]
/// # Safety
///
/// This function is called by the JVM with a valid `JavaVM*` pointer during
/// native library load. The pointer must be non-null and must outlive this call.
pub unsafe extern "system" fn JNI_OnLoad(
    vm: *mut jni::sys::JavaVM,
    _reserved: *mut c_void,
) -> jint {
    android_logger::init_once(
        android_logger::Config::default()
            .with_max_level(log::LevelFilter::Warn)
            .with_tag("ImageDecoder"),
    );

    let vm = unsafe { jni::vm::JavaVM::from_raw(vm) };
    let result = vm.attach_current_thread(|env| -> jni::errors::Result<()> {
        unsafe {
            let cls = env.find_class(jni_str!("tachiyomi/decoder/ImageDecoder"))?;
            IMAGE_DECODER_CLS = env.new_global_ref(&cls)?.into_raw();
            IMAGE_DECODER_CTOR = env
                .get_method_id(&cls, jni_str!("<init>"), jni_sig!("(JII)V"))?
                .into_raw();

            let cls2 = env.find_class(jni_str!("tachiyomi/decoder/ImageType"))?;
            IMAGE_TYPE_CLS = env.new_global_ref(&cls2)?.into_raw();
            IMAGE_TYPE_CTOR = env
                .get_method_id(&cls2, jni_str!("<init>"), jni_sig!("(IZ)V"))?
                .into_raw();

            CREATE_BITMAP_METHOD = env
                .get_static_method_id(
                    &cls,
                    jni_str!("createBitmap"),
                    jni_sig!("(II)Landroid/graphics/Bitmap;"),
                )?
                .into_raw();
        }
        Ok(())
    });

    match result {
        Ok(()) => jni::sys::JNI_VERSION_1_6,
        Err(e) => {
            log::error!("JNI_OnLoad failed: {e}");
            jni::sys::JNI_ERR
        }
    }
}

// ---------------------------------------------------------------------------
// Helper: read entire Java InputStream into a Vec<u8>
// ---------------------------------------------------------------------------

fn read_java_stream(env: &mut Env, stream: &JObject) -> Option<Vec<u8>> {
    const BUF_SIZE: i32 = 8192;

    let available: i32 = env
        .call_method(stream, jni_str!("available"), jni_sig!("()I"), &[])
        .ok()?
        .i()
        .ok()?;

    let reserved = if available > BUF_SIZE {
        available as usize
    } else {
        BUF_SIZE as usize * 50
    };

    let mut data: Vec<u8> = Vec::with_capacity(reserved);
    let buffer = JByteArray::new(env, BUF_SIZE as usize).ok()?;

    loop {
        let read: i32 = env
            .call_method(
                stream,
                jni_str!("read"),
                jni_sig!("([BII)I"),
                &[
                    JValue::Object(&buffer),
                    JValue::Int(0),
                    JValue::Int(BUF_SIZE),
                ],
            )
            .ok()?
            .i()
            .ok()?;

        if read < 0 {
            break;
        }

        let mut tmp = vec![0i8; read as usize];
        buffer.get_region(env, 0, &mut tmp).ok()?;
        data.extend_from_slice(unsafe {
            std::slice::from_raw_parts(tmp.as_ptr() as *const u8, tmp.len())
        });
    }

    if data.is_empty() {
        None
    } else {
        Some(data)
    }
}

// ---------------------------------------------------------------------------
// JNI exports
// ---------------------------------------------------------------------------

#[unsafe(no_mangle)]
pub extern "system" fn Java_tachiyomi_decoder_ImageDecoder_nativeNewInstance<'caller>(
    mut unowned_env: EnvUnowned<'caller>,
    _class: JClass<'caller>,
    jstream: JObject<'caller>,
    crop_borders: jboolean,
    icm_array: JByteArray<'caller>,
) -> jobject {
    unowned_env
        .with_env(|env| -> jni::errors::Result<_> {
            let data = match read_java_stream(env, &jstream) {
                Some(d) => d,
                None => return Ok(std::ptr::null_mut()),
            };

            let target_profile: Option<Vec<u8>> = if !icm_array.is_null() {
                let len = icm_array.len(env)?;
                if len > 0 {
                    let mut buf = vec![0i8; len];
                    icm_array.get_region(env, 0, &mut buf)?;
                    Some(unsafe {
                        std::slice::from_raw_parts(buf.as_ptr() as *const u8, buf.len()).to_vec()
                    })
                } else {
                    None
                }
            } else {
                None
            };

            let decoder = match decode::new_decoder(data, crop_borders, target_profile.as_deref()) {
                Ok(d) => d,
                Err(e) => {
                    log::error!("Failed to create decoder: {e}");
                    return Ok(std::ptr::null_mut());
                }
            };

            let bounds = decoder.info().bounds;
            let ptr = Box::into_raw(Box::new(decoder)) as jlong;

            let cls = unsafe { JClass::from_raw(env, IMAGE_DECODER_CLS) };
            let ctor = unsafe { jni::objects::JMethodID::from_raw(IMAGE_DECODER_CTOR) };

            let obj = unsafe {
                env.new_object_unchecked(
                    &cls,
                    ctor,
                    &[
                        JValue::Long(ptr).as_jni(),
                        JValue::Int(bounds.width as jint).as_jni(),
                        JValue::Int(bounds.height as jint).as_jni(),
                    ],
                )?
            };

            Ok(obj.into_raw())
        })
        .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_tachiyomi_decoder_ImageDecoder_nativeDecode<'caller>(
    mut unowned_env: EnvUnowned<'caller>,
    _this: JObject<'caller>,
    decoder_ptr: jlong,
    sample_size: jint,
    x: jint,
    y: jint,
    width: jint,
    height: jint,
) -> jobject {
    unowned_env
        .with_env(|env| -> jni::errors::Result<_> {
            let decoder = unsafe { &*(decoder_ptr as *const Box<dyn Decoder>) };
            let bounds = decoder.info().bounds;

            let in_rect = Rect {
                x: x as u32 + bounds.x,
                y: y as u32 + bounds.y,
                width: width as u32,
                height: height as u32,
            };

            let out_rect = in_rect.downsample(sample_size as u32);
            if out_rect.width == 0 || out_rect.height == 0 {
                log::error!("Requested sample size too high");
                return Ok(std::ptr::null_mut());
            }

            let cls = unsafe { JClass::from_raw(env, IMAGE_DECODER_CLS) };
            let mid = unsafe { jni::objects::JStaticMethodID::from_raw(CREATE_BITMAP_METHOD) };

            let bitmap_val = unsafe {
                env.call_static_method_unchecked(
                    &cls,
                    mid,
                    jni::signature::ReturnType::Object,
                    &[
                        JValue::Int(out_rect.width as jint).as_jni(),
                        JValue::Int(out_rect.height as jint).as_jni(),
                    ],
                )?
            };

            let bitmap = bitmap_val.l()?;
            if bitmap.is_null() {
                log::error!("Failed to create bitmap");
                return Ok(std::ptr::null_mut());
            }

            let mut pixels_ptr: *mut c_void = std::ptr::null_mut();
            let lock_result = unsafe {
                AndroidBitmap_lockPixels(env.get_raw(), bitmap.as_raw(), &mut pixels_ptr)
            };

            if lock_result != 0 || pixels_ptr.is_null() {
                log::error!("Failed to lock pixels");
                return Ok(std::ptr::null_mut());
            }

            let pixel_count = (out_rect.width * out_rect.height) as usize;
            let out_slice =
                unsafe { std::slice::from_raw_parts_mut(pixels_ptr as *mut u8, pixel_count * 4) };

            match decoder.decode(out_slice, out_rect, in_rect, sample_size as u32) {
                Ok(()) => {}
                Err(e) => {
                    log::error!("Decode error: {e}");
                    unsafe {
                        AndroidBitmap_unlockPixels(env.get_raw(), bitmap.as_raw());
                    }
                    return Ok(std::ptr::null_mut());
                }
            }

            unsafe {
                AndroidBitmap_unlockPixels(env.get_raw(), bitmap.as_raw());
            }

            Ok(bitmap.into_raw())
        })
        .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_tachiyomi_decoder_ImageDecoder_nativeRecycle<'caller>(
    mut unowned_env: EnvUnowned<'caller>,
    _this: JObject<'caller>,
    decoder_ptr: jlong,
) {
    let _ = unowned_env.with_env(|_env| -> jni::errors::Result<()> {
        if decoder_ptr != 0 {
            unsafe {
                let _ = Box::from_raw(decoder_ptr as *mut Box<dyn Decoder>);
            }
        }
        Ok(())
    });
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_tachiyomi_decoder_ImageDecoder_nativeFindType<'caller>(
    mut unowned_env: EnvUnowned<'caller>,
    _class: JClass<'caller>,
    array: JByteArray<'caller>,
) -> jobject {
    unowned_env
        .with_env(|env| -> jni::errors::Result<_> {
            let len = array.len(env)?;
            if len < 32 {
                log::warn!("Not enough bytes to parse info");
                return Ok(std::ptr::null_mut());
            }

            let mut bytes = vec![0i8; 32];
            array.get_region(env, 0, &mut bytes)?;
            let bytes =
                unsafe { std::slice::from_raw_parts(bytes.as_ptr() as *const u8, bytes.len()) };

            let image_type = match decode::find_type(bytes) {
                Some(t) => t,
                None => {
                    log::warn!("Failed to find image type");
                    return Ok(std::ptr::null_mut());
                }
            };

            let cls = unsafe { JClass::from_raw(env, IMAGE_TYPE_CLS) };
            let ctor = unsafe { jni::objects::JMethodID::from_raw(IMAGE_TYPE_CTOR) };

            let obj = unsafe {
                env.new_object_unchecked(
                    &cls,
                    ctor,
                    &[
                        JValue::Int(image_type.format as jint).as_jni(),
                        JValue::Bool(image_type.is_animated).as_jni(),
                    ],
                )?
            };

            Ok(obj.into_raw())
        })
        .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
}
