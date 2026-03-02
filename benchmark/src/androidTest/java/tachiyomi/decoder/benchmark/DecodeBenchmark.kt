package tachiyomi.decoder.benchmark

import android.content.Context
import android.graphics.Bitmap
import android.graphics.Rect
import androidx.benchmark.BenchmarkState
import androidx.benchmark.junit4.BenchmarkRule
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import java.io.ByteArrayInputStream
import org.junit.Assume.assumeTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import tachiyomi.decoder.CppImageDecoder
import tachiyomi.decoder.ImageDecoder

/**
 * Microbenchmarks that compare the **Rust** [ImageDecoder] against the **original C++**
 * [CppImageDecoder] on every supported image format.
 *
 * Tested operations per format:
 * - Full decode (entire image at scale 1)
 * - Region decode (centre 50 % crop)
 * - Scaled decode (full image at sampleSize = 2)
 *
 * ─── Adding test images ─────────────────────────────────────────────── Place sample images in
 * `benchmark/src/androidTest/assets/`:
 *
 * test_image.jpg – JPEG test_image.png – PNG test_image.webp – WebP test_image.heif – HEIF
 * test_image.avif – AVIF test_image.jxl – JPEG XL
 *
 * If an asset is missing, the corresponding tests are skipped via [assumeTrue]. JPEG / PNG / WebP
 * images are generated automatically (512 × 512 gradient) when not present in assets.
 */
@RunWith(AndroidJUnit4::class)
class DecodeBenchmark {

    @get:Rule val benchmarkRule = BenchmarkRule()

    private lateinit var context: Context

    // Lazy-loaded raw bytes for each format.
    private val imageBytes = mutableMapOf<String, ByteArray>()

    // Formats that can be generated programmatically when missing from assets.
    private val generatableFormats = setOf("jpg", "png", "webp")

    @Before
    fun setUp() {
        context = ApplicationProvider.getApplicationContext()
        loadOrGenerate("jpg")
        loadOrGenerate("png")
        loadOrGenerate("webp")
        loadOrGenerate("heif")
        loadOrGenerate("avif")
        loadOrGenerate("jxl")
    }

    // ─── Helpers ─────────────────────────────────────────────────────────

    /** Load the test image from assets, or generate one for JPEG/PNG/WebP. */
    private fun loadOrGenerate(ext: String) {
        val assetName = "test_image.$ext"
        try {
            imageBytes[ext] = context.assets.open(assetName).use { it.readBytes() }
        } catch (_: Exception) {
            if (ext in generatableFormats) {
                imageBytes[ext] = generateTestImage(ext)
            }
        }
    }

    /** Create a 512×512 gradient bitmap and compress it to the given format. */
    private fun generateTestImage(ext: String): ByteArray {
        val size = 512
        val bitmap = Bitmap.createBitmap(size, size, Bitmap.Config.ARGB_8888)
        val pixels = IntArray(size * size)
        for (y in 0 until size) {
            for (x in 0 until size) {
                val r = (x * 255 / size)
                val g = (y * 255 / size)
                val b = ((x + y) * 127 / size).coerceAtMost(255)
                pixels[y * size + x] = (0xFF shl 24) or (r shl 16) or (g shl 8) or b
            }
        }
        bitmap.setPixels(pixels, 0, size, 0, 0, size, size)

        val format =
                when (ext) {
                    "jpg" -> Bitmap.CompressFormat.JPEG
                    "png" -> Bitmap.CompressFormat.PNG
                    "webp" -> Bitmap.CompressFormat.WEBP
                    else -> error("Cannot generate $ext images")
                }
        val stream = java.io.ByteArrayOutputStream()
        bitmap.compress(format, 90, stream)
        bitmap.recycle()
        return stream.toByteArray()
    }

    private fun hasFormat(ext: String): Boolean = imageBytes.containsKey(ext)

    private fun bytesStream(ext: String) = ByteArrayInputStream(imageBytes[ext]!!)

    /** Centre 50 % region of the image. */
    private fun centreRegion(width: Int, height: Int): Rect {
        val qw = width / 4
        val qh = height / 4
        return Rect(qw, qh, width - qw, height - qh)
    }

    // ── Helper that runs a benchmark body repeatedly ─────────────────────

    private inline fun runBenchmark(state: BenchmarkState, crossinline body: () -> Unit) {
        while (state.keepRunning()) {
            body()
        }
    }

    // =====================================================================
    //  JPEG
    // =====================================================================

    // ── full decode ──────────────────────────────────────────────────────

    @Test
    fun jpeg_fullDecode_rust() {
        assumeTrue(hasFormat("jpg"))
        runBenchmark(benchmarkRule.getState()) {
            val decoder = ImageDecoder.newInstance(bytesStream("jpg"))!!
            decoder.decode()?.recycle()
            decoder.recycle()
        }
    }

    @Test
    fun jpeg_fullDecode_cpp() {
        assumeTrue(hasFormat("jpg"))
        runBenchmark(benchmarkRule.getState()) {
            val decoder = CppImageDecoder.newInstance(bytesStream("jpg"))!!
            decoder.decode()?.recycle()
            decoder.recycle()
        }
    }

    // ── region decode ────────────────────────────────────────────────────

    @Test
    fun jpeg_regionDecode_rust() {
        assumeTrue(hasFormat("jpg"))
        runBenchmark(benchmarkRule.getState()) {
            val decoder = ImageDecoder.newInstance(bytesStream("jpg"))!!
            decoder.decode(centreRegion(decoder.width, decoder.height))?.recycle()
            decoder.recycle()
        }
    }

    @Test
    fun jpeg_regionDecode_cpp() {
        assumeTrue(hasFormat("jpg"))
        runBenchmark(benchmarkRule.getState()) {
            val decoder = CppImageDecoder.newInstance(bytesStream("jpg"))!!
            decoder.decode(centreRegion(decoder.width, decoder.height))?.recycle()
            decoder.recycle()
        }
    }

    // ── scaled decode ────────────────────────────────────────────────────

    @Test
    fun jpeg_scaledDecode_rust() {
        assumeTrue(hasFormat("jpg"))
        runBenchmark(benchmarkRule.getState()) {
            val decoder = ImageDecoder.newInstance(bytesStream("jpg"))!!
            decoder.decode(sampleSize = 2)?.recycle()
            decoder.recycle()
        }
    }

    @Test
    fun jpeg_scaledDecode_cpp() {
        assumeTrue(hasFormat("jpg"))
        runBenchmark(benchmarkRule.getState()) {
            val decoder = CppImageDecoder.newInstance(bytesStream("jpg"))!!
            decoder.decode(sampleSize = 2)?.recycle()
            decoder.recycle()
        }
    }

    // =====================================================================
    //  PNG
    // =====================================================================

    @Test
    fun png_fullDecode_rust() {
        assumeTrue(hasFormat("png"))
        runBenchmark(benchmarkRule.getState()) {
            val decoder = ImageDecoder.newInstance(bytesStream("png"))!!
            decoder.decode()?.recycle()
            decoder.recycle()
        }
    }

    @Test
    fun png_fullDecode_cpp() {
        assumeTrue(hasFormat("png"))
        runBenchmark(benchmarkRule.getState()) {
            val decoder = CppImageDecoder.newInstance(bytesStream("png"))!!
            decoder.decode()?.recycle()
            decoder.recycle()
        }
    }

    @Test
    fun png_regionDecode_rust() {
        assumeTrue(hasFormat("png"))
        runBenchmark(benchmarkRule.getState()) {
            val decoder = ImageDecoder.newInstance(bytesStream("png"))!!
            decoder.decode(centreRegion(decoder.width, decoder.height))?.recycle()
            decoder.recycle()
        }
    }

    @Test
    fun png_regionDecode_cpp() {
        assumeTrue(hasFormat("png"))
        runBenchmark(benchmarkRule.getState()) {
            val decoder = CppImageDecoder.newInstance(bytesStream("png"))!!
            decoder.decode(centreRegion(decoder.width, decoder.height))?.recycle()
            decoder.recycle()
        }
    }

    @Test
    fun png_scaledDecode_rust() {
        assumeTrue(hasFormat("png"))
        runBenchmark(benchmarkRule.getState()) {
            val decoder = ImageDecoder.newInstance(bytesStream("png"))!!
            decoder.decode(sampleSize = 2)?.recycle()
            decoder.recycle()
        }
    }

    @Test
    fun png_scaledDecode_cpp() {
        assumeTrue(hasFormat("png"))
        runBenchmark(benchmarkRule.getState()) {
            val decoder = CppImageDecoder.newInstance(bytesStream("png"))!!
            decoder.decode(sampleSize = 2)?.recycle()
            decoder.recycle()
        }
    }

    // =====================================================================
    //  WebP
    // =====================================================================

    @Test
    fun webp_fullDecode_rust() {
        assumeTrue(hasFormat("webp"))
        runBenchmark(benchmarkRule.getState()) {
            val decoder = ImageDecoder.newInstance(bytesStream("webp"))!!
            decoder.decode()?.recycle()
            decoder.recycle()
        }
    }

    @Test
    fun webp_fullDecode_cpp() {
        assumeTrue(hasFormat("webp"))
        runBenchmark(benchmarkRule.getState()) {
            val decoder = CppImageDecoder.newInstance(bytesStream("webp"))!!
            decoder.decode()?.recycle()
            decoder.recycle()
        }
    }

    @Test
    fun webp_regionDecode_rust() {
        assumeTrue(hasFormat("webp"))
        runBenchmark(benchmarkRule.getState()) {
            val decoder = ImageDecoder.newInstance(bytesStream("webp"))!!
            decoder.decode(centreRegion(decoder.width, decoder.height))?.recycle()
            decoder.recycle()
        }
    }

    @Test
    fun webp_regionDecode_cpp() {
        assumeTrue(hasFormat("webp"))
        runBenchmark(benchmarkRule.getState()) {
            val decoder = CppImageDecoder.newInstance(bytesStream("webp"))!!
            decoder.decode(centreRegion(decoder.width, decoder.height))?.recycle()
            decoder.recycle()
        }
    }

    @Test
    fun webp_scaledDecode_rust() {
        assumeTrue(hasFormat("webp"))
        runBenchmark(benchmarkRule.getState()) {
            val decoder = ImageDecoder.newInstance(bytesStream("webp"))!!
            decoder.decode(sampleSize = 2)?.recycle()
            decoder.recycle()
        }
    }

    @Test
    fun webp_scaledDecode_cpp() {
        assumeTrue(hasFormat("webp"))
        runBenchmark(benchmarkRule.getState()) {
            val decoder = CppImageDecoder.newInstance(bytesStream("webp"))!!
            decoder.decode(sampleSize = 2)?.recycle()
            decoder.recycle()
        }
    }

    // =====================================================================
    //  HEIF  (both use libheif under the hood — compares JNI / wrapper overhead)
    // =====================================================================

    @Test
    fun heif_fullDecode_rust() {
        assumeTrue(hasFormat("heif"))
        runBenchmark(benchmarkRule.getState()) {
            val decoder = ImageDecoder.newInstance(bytesStream("heif"))!!
            decoder.decode()?.recycle()
            decoder.recycle()
        }
    }

    @Test
    fun heif_fullDecode_cpp() {
        assumeTrue(hasFormat("heif"))
        runBenchmark(benchmarkRule.getState()) {
            val decoder = CppImageDecoder.newInstance(bytesStream("heif"))!!
            decoder.decode()?.recycle()
            decoder.recycle()
        }
    }

    @Test
    fun heif_regionDecode_rust() {
        assumeTrue(hasFormat("heif"))
        runBenchmark(benchmarkRule.getState()) {
            val decoder = ImageDecoder.newInstance(bytesStream("heif"))!!
            decoder.decode(centreRegion(decoder.width, decoder.height))?.recycle()
            decoder.recycle()
        }
    }

    @Test
    fun heif_regionDecode_cpp() {
        assumeTrue(hasFormat("heif"))
        runBenchmark(benchmarkRule.getState()) {
            val decoder = CppImageDecoder.newInstance(bytesStream("heif"))!!
            decoder.decode(centreRegion(decoder.width, decoder.height))?.recycle()
            decoder.recycle()
        }
    }

    @Test
    fun heif_scaledDecode_rust() {
        assumeTrue(hasFormat("heif"))
        runBenchmark(benchmarkRule.getState()) {
            val decoder = ImageDecoder.newInstance(bytesStream("heif"))!!
            decoder.decode(sampleSize = 2)?.recycle()
            decoder.recycle()
        }
    }

    @Test
    fun heif_scaledDecode_cpp() {
        assumeTrue(hasFormat("heif"))
        runBenchmark(benchmarkRule.getState()) {
            val decoder = CppImageDecoder.newInstance(bytesStream("heif"))!!
            decoder.decode(sampleSize = 2)?.recycle()
            decoder.recycle()
        }
    }

    // =====================================================================
    //  AVIF  (both use libheif under the hood — compares JNI / wrapper overhead)
    // =====================================================================

    @Test
    fun avif_fullDecode_rust() {
        assumeTrue(hasFormat("avif"))
        runBenchmark(benchmarkRule.getState()) {
            val decoder = ImageDecoder.newInstance(bytesStream("avif"))!!
            decoder.decode()?.recycle()
            decoder.recycle()
        }
    }

    @Test
    fun avif_fullDecode_cpp() {
        assumeTrue(hasFormat("avif"))
        runBenchmark(benchmarkRule.getState()) {
            val decoder = CppImageDecoder.newInstance(bytesStream("avif"))!!
            decoder.decode()?.recycle()
            decoder.recycle()
        }
    }

    @Test
    fun avif_regionDecode_rust() {
        assumeTrue(hasFormat("avif"))
        runBenchmark(benchmarkRule.getState()) {
            val decoder = ImageDecoder.newInstance(bytesStream("avif"))!!
            decoder.decode(centreRegion(decoder.width, decoder.height))?.recycle()
            decoder.recycle()
        }
    }

    @Test
    fun avif_regionDecode_cpp() {
        assumeTrue(hasFormat("avif"))
        runBenchmark(benchmarkRule.getState()) {
            val decoder = CppImageDecoder.newInstance(bytesStream("avif"))!!
            decoder.decode(centreRegion(decoder.width, decoder.height))?.recycle()
            decoder.recycle()
        }
    }

    @Test
    fun avif_scaledDecode_rust() {
        assumeTrue(hasFormat("avif"))
        runBenchmark(benchmarkRule.getState()) {
            val decoder = ImageDecoder.newInstance(bytesStream("avif"))!!
            decoder.decode(sampleSize = 2)?.recycle()
            decoder.recycle()
        }
    }

    @Test
    fun avif_scaledDecode_cpp() {
        assumeTrue(hasFormat("avif"))
        runBenchmark(benchmarkRule.getState()) {
            val decoder = CppImageDecoder.newInstance(bytesStream("avif"))!!
            decoder.decode(sampleSize = 2)?.recycle()
            decoder.recycle()
        }
    }

    // =====================================================================
    //  JXL (JPEG XL)
    // =====================================================================

    @Test
    fun jxl_fullDecode_rust() {
        assumeTrue(hasFormat("jxl"))
        runBenchmark(benchmarkRule.getState()) {
            val decoder = ImageDecoder.newInstance(bytesStream("jxl"))!!
            decoder.decode()?.recycle()
            decoder.recycle()
        }
    }

    @Test
    fun jxl_fullDecode_cpp() {
        assumeTrue(hasFormat("jxl"))
        runBenchmark(benchmarkRule.getState()) {
            val decoder = CppImageDecoder.newInstance(bytesStream("jxl"))!!
            decoder.decode()?.recycle()
            decoder.recycle()
        }
    }

    @Test
    fun jxl_regionDecode_rust() {
        assumeTrue(hasFormat("jxl"))
        runBenchmark(benchmarkRule.getState()) {
            val decoder = ImageDecoder.newInstance(bytesStream("jxl"))!!
            decoder.decode(centreRegion(decoder.width, decoder.height))?.recycle()
            decoder.recycle()
        }
    }

    @Test
    fun jxl_regionDecode_cpp() {
        assumeTrue(hasFormat("jxl"))
        runBenchmark(benchmarkRule.getState()) {
            val decoder = CppImageDecoder.newInstance(bytesStream("jxl"))!!
            decoder.decode(centreRegion(decoder.width, decoder.height))?.recycle()
            decoder.recycle()
        }
    }

    @Test
    fun jxl_scaledDecode_rust() {
        assumeTrue(hasFormat("jxl"))
        runBenchmark(benchmarkRule.getState()) {
            val decoder = ImageDecoder.newInstance(bytesStream("jxl"))!!
            decoder.decode(sampleSize = 2)?.recycle()
            decoder.recycle()
        }
    }

    @Test
    fun jxl_scaledDecode_cpp() {
        assumeTrue(hasFormat("jxl"))
        runBenchmark(benchmarkRule.getState()) {
            val decoder = CppImageDecoder.newInstance(bytesStream("jxl"))!!
            decoder.decode(sampleSize = 2)?.recycle()
            decoder.recycle()
        }
    }
}
