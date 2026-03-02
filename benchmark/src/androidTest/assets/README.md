# Benchmark Test Images

Place sample images here for benchmarking.  JPEG, PNG, and WebP test images
are generated automatically (512×512 gradient) when not present, but you can
provide your own for more realistic benchmarks.

## Required filenames

| File              | Format   | Notes                                      |
|-------------------|----------|--------------------------------------------|
| `test_image.jpg`  | JPEG     | Auto-generated if missing                  |
| `test_image.png`  | PNG      | Auto-generated if missing                  |
| `test_image.webp` | WebP     | Auto-generated if missing                  |
| `test_image.heif` | HEIF     | Must be provided (skipped if missing)      |
| `test_image.avif` | AVIF     | Must be provided (skipped if missing)      |
| `test_image.jxl`  | JPEG XL  | Must be provided (skipped if missing)      |

## Recommended test images

For best benchmark results, use images that are:

- **512×512** or **1024×1024** pixels (large enough to be meaningful)
- **Realistic** (photos or manga pages rather than solid colours)
- **Representative** of the actual workload (manga/comic pages)

You can convert between formats using ImageMagick:

```bash
# Convert a JPEG to HEIF / AVIF / JXL
convert input.jpg test_image.heif
convert input.jpg test_image.avif
cjxl input.jpg test_image.jxl
```
