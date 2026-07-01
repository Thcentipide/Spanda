use image::{DynamicImage, GenericImageView};

/// YCbCr representation of an image with double-precision floating-point channels.
#[derive(Debug, Clone, PartialEq)]
pub struct YCbCrImage {
    pub width: u32,
    pub height: u32,
    pub y: Vec<f64>,
    pub cb: Vec<f64>,
    pub cr: Vec<f64>,
}

/// Converts an RGB image buffer to a YCbCr working representation.
/// Uses standard full-range BT.601 conversion formulas.
pub fn rgb_to_ycbcr(image: &DynamicImage) -> YCbCrImage {
    let (width, height) = image.dimensions();
    let size = (width * height) as usize;
    let mut y = Vec::with_capacity(size);
    let mut cb = Vec::with_capacity(size);
    let mut cr = Vec::with_capacity(size);

    let rgb = image.to_rgb8();
    for pixel in rgb.pixels() {
        let r = pixel[0] as f64;
        let g = pixel[1] as f64;
        let b = pixel[2] as f64;

        // BT.601 conversion formulas
        let y_val = 0.299 * r + 0.587 * g + 0.114 * b;
        let cb_val = -0.168736 * r - 0.331264 * g + 0.5 * b + 128.0;
        let cr_val = 0.5 * r - 0.418688 * g - 0.081312 * b + 128.0;

        y.push(y_val);
        cb.push(cb_val);
        cr.push(cr_val);
    }

    YCbCrImage { width, height, y, cb, cr }
}

/// Converts a YCbCr image buffer back to a standard RGB image.
/// Uses standard full-range BT.601 conversion formulas.
pub fn ycbcr_to_rgb(ycbcr: &YCbCrImage) -> DynamicImage {
    let mut imgbuf = image::RgbImage::new(ycbcr.width, ycbcr.height);
    for (i, pixel) in imgbuf.pixels_mut().enumerate() {
        let y_val = ycbcr.y[i];
        let cb_val = ycbcr.cb[i] - 128.0;
        let cr_val = ycbcr.cr[i] - 128.0;

        let r = y_val + 1.402 * cr_val;
        let g = y_val - 0.344136 * cb_val - 0.714136 * cr_val;
        let b = y_val + 1.772 * cb_val;

        pixel[0] = r.clamp(0.0, 255.0).round() as u8;
        pixel[1] = g.clamp(0.0, 255.0).round() as u8;
        pixel[2] = b.clamp(0.0, 255.0).round() as u8;
    }
    DynamicImage::ImageRgb8(imgbuf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::RgbImage;

    #[test]
    fn test_color_round_trip() {
        // Create a 10x10 RGB image with gradient colors
        let mut rgb = RgbImage::new(10, 10);
        for (x, y, pixel) in rgb.enumerate_pixels_mut() {
            pixel[0] = (x * 25) as u8;
            pixel[1] = (y * 25) as u8;
            pixel[2] = ((x + y) * 12) as u8;
        }

        let original = DynamicImage::ImageRgb8(rgb);
        let ycbcr = rgb_to_ycbcr(&original);
        let reconstructed = ycbcr_to_rgb(&ycbcr);

        let orig_rgb = original.to_rgb8();
        let recon_rgb = reconstructed.to_rgb8();

        // Check that reconstruction matches the original closely
        for (p1, p2) in orig_rgb.pixels().zip(recon_rgb.pixels()) {
            assert!((p1[0] as i16 - p2[0] as i16).abs() <= 1);
            assert!((p1[1] as i16 - p2[1] as i16).abs() <= 1);
            assert!((p1[2] as i16 - p2[2] as i16).abs() <= 1);
        }
    }
}
