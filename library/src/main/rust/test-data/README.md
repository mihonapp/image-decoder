# Test Data

Place sample images here for the integration test suite. The tests will
gracefully skip when a file is missing, so CI can run even without the
full media set.

## Required files

| File            | Description                                        |
|-----------------|----------------------------------------------------|
| `sample.jpg`    | Any JPEG image (≥ 100×100 recommended)             |
| `sample.png`    | Any PNG image                                      |
| `sample.webp`   | Any WebP image                                     |
| `sample.jxl`    | Any JPEG XL image                                  |
| `sample.heif`   | Any HEIF image (optional, needs libheif at runtime)|
| `bordered.jpg`  | JPEG with white/black borders for crop-border test |

Add more here when adding tests, document their purpose, and update the test suite to use them.

You can generate the above test images with ImageMagick:

```sh
# 200×300 red rectangle on white, with 20px border
convert -size 200x300 xc:white -fill red -draw "rectangle 20,20 180,280" bordered.jpg
convert -size 100x100 xc:blue sample.jpg
convert -size 100x100 xc:green sample.png
convert -size 100x100 xc:red sample.heif
cwebp sample.png -o sample.webp
# JXL requires cjxl from libjxl
cjxl sample.png sample.jxl
```
