use crate::types::Rect;

/// A line will be considered as having content if 0.25% of it is filled.
const FILLED_RATIO_LIMIT: f64 = 0.0025;

/// When the threshold is closer to 1, less content will be cropped.
const THRESHOLD: f64 = 0.75;

const THRESHOLD_FOR_BLACK: u8 = (255.0 * THRESHOLD) as u8;
const THRESHOLD_FOR_WHITE: u8 = (255.0 - 255.0 * THRESHOLD) as u8;

#[inline]
fn is_black_pixel(pixels: &[u8], width: u32, x: u32, y: u32) -> bool {
    let idx = (y * width + x) as usize;
    pixels[idx] < THRESHOLD_FOR_BLACK
}

#[inline]
fn is_white_pixel(pixels: &[u8], width: u32, x: u32, y: u32) -> bool {
    let idx = (y * width + x) as usize;
    pixels[idx] > THRESHOLD_FOR_WHITE
}

/// Return the first x position where there is a substantial amount of fill,
/// starting the search from the left.
fn find_border_left(pixels: &[u8], width: u32, _height: u32, top: u32, bottom: u32) -> u32 {
    let filled_limit = ((bottom - top) as f64 * FILLED_RATIO_LIMIT / 2.0).round() as u32;

    // Scan first column to detect dominant color
    let mut white_pixels = 0u32;
    let mut black_pixels = 0u32;

    let mut y = top;
    while y < bottom {
        if is_black_pixel(pixels, width, 0, y) {
            black_pixels += 1;
        } else if is_white_pixel(pixels, width, 0, y) {
            white_pixels += 1;
        }
        y += 2;
    }

    let detect_func: fn(&[u8], u32, u32, u32) -> bool =
        if white_pixels > filled_limit && black_pixels > filled_limit {
            return 0;
        } else if black_pixels > filled_limit {
            is_white_pixel
        } else {
            is_black_pixel
        };

    for x in 1..width {
        let mut filled_count = 0u32;
        let mut y = top;
        while y < bottom {
            if detect_func(pixels, width, x, y) {
                filled_count += 1;
            }
            y += 2;
        }
        if filled_count > filled_limit {
            return x;
        }
    }

    0
}

/// Return the first x position where there is a substantial amount of fill,
/// starting the search from the right.
fn find_border_right(pixels: &[u8], width: u32, _height: u32, top: u32, bottom: u32) -> u32 {
    let filled_limit = ((bottom - top) as f64 * FILLED_RATIO_LIMIT / 2.0).round() as u32;

    let last_x = width - 1;
    let mut white_pixels = 0u32;
    let mut black_pixels = 0u32;

    let mut y = top;
    while y < bottom {
        if is_black_pixel(pixels, width, last_x, y) {
            black_pixels += 1;
        } else if is_white_pixel(pixels, width, last_x, y) {
            white_pixels += 1;
        }
        y += 2;
    }

    let detect_func: fn(&[u8], u32, u32, u32) -> bool =
        if white_pixels > filled_limit && black_pixels > filled_limit {
            return width;
        } else if black_pixels > filled_limit {
            is_white_pixel
        } else {
            is_black_pixel
        };

    if width < 3 {
        return width;
    }
    for x in (1..=(width - 2)).rev() {
        let mut filled_count = 0u32;
        let mut y = top;
        while y < bottom {
            if detect_func(pixels, width, x, y) {
                filled_count += 1;
            }
            y += 2;
        }
        if filled_count > filled_limit {
            return x + 1;
        }
    }

    width
}

/// Return the first y position where there is a substantial amount of fill,
/// starting the search from the top.
fn find_border_top(pixels: &[u8], width: u32, height: u32) -> u32 {
    let filled_limit = (width as f64 * FILLED_RATIO_LIMIT / 2.0).round() as u32;

    let mut white_pixels = 0u32;
    let mut black_pixels = 0u32;

    let mut x = 0u32;
    while x < width {
        if is_black_pixel(pixels, width, x, 0) {
            black_pixels += 1;
        } else if is_white_pixel(pixels, width, x, 0) {
            white_pixels += 1;
        }
        x += 2;
    }

    let detect_func: fn(&[u8], u32, u32, u32) -> bool =
        if white_pixels > filled_limit && black_pixels > filled_limit {
            return 0;
        } else if black_pixels > filled_limit {
            is_white_pixel
        } else {
            is_black_pixel
        };

    for y in 1..height {
        let mut filled_count = 0u32;
        let mut x = 0u32;
        while x < width {
            if detect_func(pixels, width, x, y) {
                filled_count += 1;
            }
            x += 2;
        }
        if filled_count > filled_limit {
            return y;
        }
    }

    0
}

/// Return the first y position where there is a substantial amount of fill,
/// starting the search from the bottom.
fn find_border_bottom(pixels: &[u8], width: u32, height: u32) -> u32 {
    let filled_limit = (width as f64 * FILLED_RATIO_LIMIT / 2.0).round() as u32;

    let last_y = height - 1;
    let mut white_pixels = 0u32;
    let mut black_pixels = 0u32;

    let mut x = 0u32;
    while x < width {
        if is_black_pixel(pixels, width, x, last_y) {
            black_pixels += 1;
        } else if is_white_pixel(pixels, width, x, last_y) {
            white_pixels += 1;
        }
        x += 2;
    }

    let detect_func: fn(&[u8], u32, u32, u32) -> bool =
        if white_pixels > filled_limit && black_pixels > filled_limit {
            return height;
        } else if black_pixels > filled_limit {
            is_white_pixel
        } else {
            is_black_pixel
        };

    if height < 3 {
        return height;
    }
    for y in (1..=(height - 2)).rev() {
        let mut filled_count = 0u32;
        let mut x = 0u32;
        while x < width {
            if detect_func(pixels, width, x, y) {
                filled_count += 1;
            }
            x += 2;
        }
        if filled_count > filled_limit {
            return y + 1;
        }
    }

    height
}

/// Finds the borders of the image. Operates on a single-component (grayscale)
/// buffer of size `width * height`.
pub fn find_borders(pixels: &[u8], width: u32, height: u32) -> Rect {
    let top = find_border_top(pixels, width, height);
    let bottom = find_border_bottom(pixels, width, height);
    let left = find_border_left(pixels, width, height, top, bottom);
    let right = find_border_right(pixels, width, height, top, bottom);

    Rect {
        x: left,
        y: top,
        width: right - left,
        height: bottom - top,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: create a white image with a dark content rectangle drawn inside.
    fn make_bordered_image(width: u32, height: u32, content: Rect) -> Vec<u8> {
        let mut pixels = vec![255u8; (width * height) as usize]; // all white

        for y in content.y..(content.y + content.height) {
            for x in content.x..(content.x + content.width) {
                pixels[(y * width + x) as usize] = 0; // black
            }
        }
        pixels
    }

    #[test]
    fn no_borders_needed() {
        // Content fills the entire image
        let w = 200;
        let h = 300;
        let pixels = vec![0u8; (w * h) as usize]; // all black
        let bounds = find_borders(&pixels, w, h);
        assert_eq!(bounds, Rect::full(w, h));
    }

    #[test]
    fn symmetric_white_borders() {
        let w = 200u32;
        let h = 300u32;
        let content = Rect::new(20, 30, 160, 240);
        let pixels = make_bordered_image(w, h, content);
        let bounds = find_borders(&pixels, w, h);

        // The algorithm may not find the exact pixel; allow ±2 px tolerance.
        assert!((bounds.x as i32 - content.x as i32).unsigned_abs() <= 2);
        assert!((bounds.y as i32 - content.y as i32).unsigned_abs() <= 2);
        assert!(
            ((bounds.x + bounds.width) as i32 - (content.x + content.width) as i32).unsigned_abs()
                <= 2
        );
        assert!(
            ((bounds.y + bounds.height) as i32 - (content.y + content.height) as i32)
                .unsigned_abs()
                <= 2
        );
    }
}
