/// Axis-aligned rectangle used for cropping / region decoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl Rect {
    pub fn new(x: u32, y: u32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// Full image bounds starting at origin.
    pub fn full(width: u32, height: u32) -> Self {
        Self {
            x: 0,
            y: 0,
            width,
            height,
        }
    }

    /// Divide all coordinates by `scale`.
    pub fn downsample(self, scale: u32) -> Self {
        if scale == 1 {
            return self;
        }
        Self {
            x: self.x / scale,
            y: self.y / scale,
            width: self.width / scale,
            height: self.height / scale,
        }
    }

    /// Multiply all coordinates by `scale`.
    pub fn upsample(self, scale: u32) -> Self {
        if scale == 1 {
            return self;
        }
        Self {
            x: self.x * scale,
            y: self.y * scale,
            width: self.width * scale,
            height: self.height * scale,
        }
    }
}

/// Information gathered from an image header and optional border-crop pass.
#[derive(Debug, Clone)]
pub struct ImageInfo {
    /// Original width of the image.
    pub image_width: u32,
    /// Original height of the image.
    pub image_height: u32,
    /// Whether the file contains animation frames.
    pub is_animated: bool,
    /// Usable content bounds (may be smaller than full image when borders were
    /// cropped).
    pub bounds: Rect,
}

/// Supported image formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Format {
    Jpeg = 0,
    Png = 1,
    Webp = 2,
    Gif = 3,
    Heif = 4,
    Avif = 5,
    Jxl = 6,
}

/// Result of detecting the image type from the first bytes.
#[derive(Debug, Clone, Copy)]
pub struct ImageType {
    pub format: Format,
    pub is_animated: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_downsample() {
        let r = Rect::new(100, 200, 400, 800);
        let d = r.downsample(2);
        assert_eq!(d, Rect::new(50, 100, 200, 400));
    }

    #[test]
    fn rect_upsample() {
        let r = Rect::new(50, 100, 200, 400);
        let u = r.upsample(2);
        assert_eq!(u, Rect::new(100, 200, 400, 800));
    }

    #[test]
    fn rect_downsample_identity() {
        let r = Rect::new(10, 20, 30, 40);
        assert_eq!(r.downsample(1), r);
    }

    #[test]
    fn rect_full() {
        let r = Rect::full(1920, 1080);
        assert_eq!(r, Rect::new(0, 0, 1920, 1080));
    }
}
