use num_complex::Complex;
use rustfft::FftPlanner;
use crate::color::YCbCrImage;

/// Represents the 2D DFT spectrum of the Y channel along with preserved Cb and Cr color channels.
pub struct DftImage {
    pub width: usize,
    pub height: usize,
    pub data: Vec<Complex<f64>>, // Row-major 2D DFT coefficients of the Y channel
    pub cb: Vec<f64>,            // Preserved Cb color channel
    pub cr: Vec<f64>,            // Preserved Cr color channel
}

/// Helper function to perform matrix transposition.
fn transpose(data: &[Complex<f64>], rows: usize, cols: usize) -> Vec<Complex<f64>> {
    let mut transposed = vec![Complex::default(); rows * cols];
    for r in 0..rows {
        for c in 0..cols {
            transposed[c * rows + r] = data[r * cols + c];
        }
    }
    transposed
}

/// Performs a forward 2D Fast Fourier Transform (FFT) on the Y channel.
pub fn compute_2d_dft(image: &YCbCrImage) -> DftImage {
    let w = image.width as usize;
    let h = image.height as usize;
    
    // Copy Y channel values as the real part of the complex buffer
    let mut buffer: Vec<Complex<f64>> = image.y.iter().map(|&val| Complex::new(val, 0.0)).collect();
    
    let mut planner = FftPlanner::new();
    
    // 1. Perform 1D FFT on each of the H rows of length W
    let fft_row = planner.plan_fft_forward(w);
    for r in 0..h {
        let row_start = r * w;
        let row_end = row_start + w;
        fft_row.process(&mut buffer[row_start..row_end]);
    }
    
    // 2. Transpose buffer to perform column FFTs as row FFTs
    let mut transposed = transpose(&buffer, h, w);
    
    // 3. Perform 1D FFT on each of the W columns of length H
    let fft_col = planner.plan_fft_forward(h);
    for c in 0..w {
        let col_start = c * h;
        let col_end = col_start + h;
        fft_col.process(&mut transposed[col_start..col_end]);
    }
    
    // 4. Transpose back to H x W
    let final_data = transpose(&transposed, w, h);
    
    DftImage {
        width: w,
        height: h,
        data: final_data,
        cb: image.cb.clone(),
        cr: image.cr.clone(),
    }
}

/// Performs an inverse 2D Fast Fourier Transform (IFFT) on the DFT representation.
pub fn compute_2d_idft(dft: &DftImage) -> YCbCrImage {
    let w = dft.width;
    let h = dft.height;
    
    let mut buffer = dft.data.clone();
    let mut planner = FftPlanner::new();
    
    // 1. Perform 1D IFFT on each of the H rows of length W
    let ifft_row = planner.plan_fft_inverse(w);
    for r in 0..h {
        let row_start = r * w;
        let row_end = row_start + w;
        ifft_row.process(&mut buffer[row_start..row_end]);
    }
    
    // 2. Transpose buffer to perform column IFFTs
    let mut transposed = transpose(&buffer, h, w);
    
    // 3. Perform 1D IFFT on each of the W columns of length H
    let ifft_col = planner.plan_fft_inverse(h);
    for c in 0..w {
        let col_start = c * h;
        let col_end = col_start + h;
        ifft_col.process(&mut transposed[col_start..col_end]);
    }
    
    // 4. Transpose back to H x W
    let final_complex = transpose(&transposed, w, h);
    
    // 5. Reconstruct Y channel by extracting the real part and normalizing by W * H
    let norm = (w * h) as f64;
    let y: Vec<f64> = final_complex.iter().map(|c| c.re / norm).collect();
    
    YCbCrImage {
        width: w as u32,
        height: h as u32,
        y,
        cb: dft.cb.clone(),
        cr: dft.cr.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dft_idft_round_trip() {
        let w = 16;
        let h = 16;
        let size = w * h;
        
        let mut y = Vec::with_capacity(size);
        for i in 0..size {
            // Generate some wave-like pattern
            let val = ((i as f64 * 0.1).sin() + (i as f64 * 0.25).cos()) * 100.0 + 128.0;
            y.push(val);
        }
        
        let original = YCbCrImage {
            width: w as u32,
            height: h as u32,
            y,
            cb: vec![128.0; size],
            cr: vec![128.0; size],
        };
        
        let dft = compute_2d_dft(&original);
        assert_eq!(dft.width, w);
        assert_eq!(dft.height, h);
        assert_eq!(dft.data.len(), size);
        
        let reconstructed = compute_2d_idft(&dft);
        assert_eq!(reconstructed.width, original.width);
        assert_eq!(reconstructed.height, original.height);
        
        // Assert reconstructed Y matches original to within 1e-9 tolerance
        for i in 0..size {
            let diff = (original.y[i] - reconstructed.y[i]).abs();
            assert!(diff < 1e-9, "Mismatch at index {}, original={}, reconstructed={}, diff={}", i, original.y[i], reconstructed.y[i], diff);
        }
    }
}
