package tachiyomi.decoder

/**
 * Configuration for the native image decoder backend.
 *
 * The consuming application must call [setBackend] **before** the first use of [ImageDecoder] (e.g.
 * in `Application.onCreate()`), because the native library is loaded in the class initialiser and
 * cannot be changed afterwards.
 *
 * ```kotlin
 * // In your Application class:
 * override fun onCreate() {
 *     super.onCreate()
 *     ImageDecoderConfig.setBackend(ImageDecoderConfig.Backend.RUST) // opt-in
 * }
 * ```
 *
 * If [setBackend] is never called the **C++** backend is used (the historical default), so existing
 * consumers keep working without any changes.
 */
object ImageDecoderConfig {

    /** Available native implementations. */
    enum class Backend {
        /** Original C++ implementation (default). */
        CPP,
        /** New Rust implementation. */
        RUST,
    }

    @Volatile private var _backend: Backend = Backend.CPP

    /** The currently selected backend. */
    val backend: Backend
        get() = _backend

    /**
     * Select the native backend to use.
     *
     * Must be called before any [ImageDecoder] API is invoked — typically in
     * `Application.onCreate()`. Calling it after the native library has already been loaded has no
     * effect.
     */
    fun setBackend(backend: Backend) {
        _backend = backend
    }
}
