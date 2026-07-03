use image::{DynamicImage, imageops::FilterType};
use serde::{Serialize, Deserialize};
use crate::dct::dct_2d_32x32;

/// Represents a 256-bit perceptual hash (32 bytes).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PHash256(pub [u8; 32]);

/// Computes the 256-bit perceptual hash (pHash) of an image or tile.
/// Algorithm (pHash256):
/// 1. Convert input to grayscale.
/// 2. Resize to exactly 32x32 using bilinear interpolation.
/// 3. Compute the 2D DCT of the 32x32 matrix.
/// 4. Extract the top-left 16x16 sub-block of DCT coefficients, excluding DC term at (0,0).
/// 5. Compute the arithmetic mean of these 255 values.
/// 6. Set bit k to 1 if coeff_k > mean, else 0.
/// 7. Pad the 256th bit (bit 255) with 0.
/// 8. Pack into 32 bytes MSB-first within each byte.
pub fn compute_phash256(image: &DynamicImage) -> PHash256 {
    // 1. Convert to grayscale
    let gray_img = image.to_luma8();
    
    // 2. Resize to 32x32 using bilinear interpolation (Triangle filter)
    let resized = DynamicImage::ImageLuma8(gray_img).resize_exact(32, 32, FilterType::Triangle);
    let luma = resized.to_luma8();

    let mut pixels = [[0.0; 32]; 32];
    for y in 0..32 {
        for x in 0..32 {
            pixels[y as usize][x as usize] = luma.get_pixel(x, y)[0] as f64;
        }
    }

    // 3. Compute 2D DCT of the 32x32 matrix
    let dct = dct_2d_32x32(&pixels);

    // 4. Extract top-left 16x16 sub-block excluding DC term at (0,0)
    let mut coefficients = Vec::with_capacity(255);
    for u in 0..16 {
        for v in 0..16 {
            if u == 0 && v == 0 {
                continue;
            }
            coefficients.push(dct[u][v]);
        }
    }

    // 5. Compute arithmetic mean
    let mean: f64 = coefficients.iter().sum::<f64>() / 255.0;

    // 6. Build the 256-bit hash with MSB-first byte packing
    let mut hash_bytes = [0u8; 32];
    for (k, &coeff) in coefficients.iter().enumerate() {
        if coeff > mean {
            let byte_idx = k / 8;
            let bit_idx = k % 8;
            hash_bytes[byte_idx] |= 1 << (7 - bit_idx); // MSB-first bit packing
        }
    }
    // Note: the 256th bit (index 255) is 0 because hash_bytes starts zeroed.

    PHash256(hash_bytes)
}

/// Computes Hamming distance between two 256-bit hashes.
pub fn hamming_distance(a: &PHash256, b: &PHash256) -> u32 {
    a.0.iter()
        .zip(b.0.iter())
        .map(|(&x, &y)| (x ^ y).count_ones())
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::RgbImage;

    #[test]
    fn test_phash_determinism() {
        let mut rgb = RgbImage::new(64, 64);
        for (x, y, pixel) in rgb.enumerate_pixels_mut() {
            pixel[0] = (x * 4) as u8;
            pixel[1] = (y * 4) as u8;
            pixel[2] = 128;
        }
        let img1 = DynamicImage::ImageRgb8(rgb.clone());
        let img2 = DynamicImage::ImageRgb8(rgb);

        let hash1 = compute_phash256(&img1);
        let hash2 = compute_phash256(&img2);
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_phash_robustness() {
        let mut rgb = RgbImage::new(128, 128);
        for (x, y, pixel) in rgb.enumerate_pixels_mut() {
            pixel[0] = ((x * y) % 256) as u8;
            pixel[1] = ((x + y) % 256) as u8;
            pixel[2] = 100;
        }
        let original = DynamicImage::ImageRgb8(rgb);
        let hash_orig = compute_phash256(&original);

        // Add small noise to the image
        let mut rgb_noisy = original.to_rgb8();
        for (idx, pixel) in rgb_noisy.pixels_mut().enumerate() {
            if idx % 10 == 0 {
                pixel[0] = pixel[0].saturating_add(5);
                pixel[1] = pixel[1].saturating_sub(5);
            }
        }
        let noisy = DynamicImage::ImageRgb8(rgb_noisy);
        let hash_noisy = compute_phash256(&noisy);

        let distance = hamming_distance(&hash_orig, &hash_noisy);
        assert!(distance < 20, "Hamming distance was {}", distance);
    }
}
