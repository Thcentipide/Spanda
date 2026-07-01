use image::{DynamicImage, imageops::FilterType};
use serde::{Serialize, Deserialize};

/// Represents a 256-bit perceptual hash (32 bytes).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PHash256(pub [u8; 32]);

/// Computes the 256-bit perceptual hash (pHash) of an image or tile.
pub fn compute_phash256(image: &DynamicImage) -> PHash256 {
    // 1. Convert to grayscale and resize to 32x32 using bilinear interpolation (Triangle filter)
    let gray_img = image.grayscale();
    let resized = gray_img.resize_exact(32, 32, FilterType::Triangle);
    let luma = resized.to_luma8();

    // Convert pixel values to f64 matrix
    let mut p = [[0.0; 32]; 32];
    for y in 0..32 {
        for x in 0..32 {
            p[y as usize][x as usize] = luma.get_pixel(x, y)[0] as f64;
        }
    }

    // Precompute cos table for 16x32 (we only need the top-left 16x16 DCT block)
    let mut cos_table = [[0.0; 32]; 16];
    for i in 0..16 {
        for x in 0..32 {
            cos_table[i][x] = (((2 * x + 1) as f64 * i as f64 * std::f64::consts::PI) / 64.0).cos();
        }
    }

    // 2. Apply 2D DCT-II to obtain the top-left 16x16 coefficients
    let mut dct = [[0.0; 16]; 16];
    for u in 0..16 {
        let cu = if u == 0 { 1.0 / 2.0_f64.sqrt() } else { 1.0 };
        for v in 0..16 {
            let cv = if v == 0 { 1.0 / 2.0_f64.sqrt() } else { 1.0 };
            
            let mut sum = 0.0;
            for y in 0..32 {
                for x in 0..32 {
                    sum += p[y][x] * cos_table[u][x] * cos_table[v][y];
                }
            }
            dct[u][v] = 0.25 * cu * cv * sum;
        }
    }

    // 3. Extract the top-left 16x16 coefficients excluding DC term D(0,0)
    let mut coeffs = Vec::with_capacity(255);
    for u in 0..16 {
        for v in 0..16 {
            if u == 0 && v == 0 {
                continue;
            }
            coeffs.push(dct[u][v]);
        }
    }

    // 4. Calculate the mean value of these 255 coefficients
    let sum_coeffs: f64 = coeffs.iter().sum();
    let mean = sum_coeffs / 255.0;

    // 5. Construct a 256-bit binary string (packed into 32 bytes)
    // Bit k is 1 if coeff > mean, 0 otherwise. Pad the 256th bit with 0.
    let mut hash_bytes = [0u8; 32];
    for (idx, &coeff) in coeffs.iter().enumerate() {
        if coeff > mean {
            let byte_idx = idx / 8;
            let bit_idx = idx % 8;
            hash_bytes[byte_idx] |= 1 << bit_idx; // LSB-first bit packing
        }
    }
    // Note: The 256th bit (at idx = 255) is already 0 as hash_bytes is initialized to 0

    PHash256(hash_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{RgbImage, DynamicImage};

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
        // Build a random image
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

        // Compare Hamming distance
        let mut distance = 0;
        for i in 0..32 {
            let diff = hash_orig.0[i] ^ hash_noisy.0[i];
            distance += diff.count_ones();
        }

        // Distance should be small (e.g. less than 10 bits changed out of 256)
        assert!(distance < 10, "Hamming distance was {}", distance);
    }
}
