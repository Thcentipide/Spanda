use num_complex::Complex;
use crate::dft::DftImage;

/// Represents the DFT spectrum in Polar Coordinates centered around the DC term.
pub struct PolarDftImage {
    pub width: usize,          // Original Cartesian image width
    pub height: usize,         // Original Cartesian image height
    pub radial_bins: usize,    // Number of radial bins (Nr)
    pub angular_bins: usize,   // Number of angular bins (Ntheta)
    pub magnitude: Vec<f64>,   // Grid of size radial_bins * angular_bins in row-major order
    pub phase: Vec<f64>,       // Grid of size radial_bins * angular_bins in row-major order
    pub cb: Vec<f64>,          // Preserved Cb color channel
    pub cr: Vec<f64>,          // Preserved Cr color channel
}

/// Maps a Cartesian DFT representation to Polar coordinates centered around the DC term.
/// Uses nearest-neighbor mapping to prevent interpolation-induced spectral attenuation.
pub fn dft_to_polar(dft: &DftImage) -> PolarDftImage {
    let w = dft.width;
    let h = dft.height;
    
    let r_max = ((w as f64 / 2.0).powi(2) + (h as f64 / 2.0).powi(2)).sqrt();
    let radial_bins = (r_max.floor() as usize) + 1;
    let angular_bins = 360;
    
    let size = radial_bins * angular_bins;
    let mut magnitude = vec![0.0; size];
    let mut phase = vec![0.0; size];
    
    for r_idx in 0..radial_bins {
        let r = (r_idx as f64) * r_max / ((radial_bins - 1) as f64);
        for theta_idx in 0..angular_bins {
            let theta = (theta_idx as f64) * 2.0 * std::f64::consts::PI / (angular_bins as f64);
            
            // Map to centered coordinates (origin is DC term)
            let x = r * theta.cos();
            let y = r * theta.sin();
            
            let w_half = w as f64 / 2.0;
            let h_half = h as f64 / 2.0;
            
            // Map to nearest neighbor if inside Nyquist zone
            let val = if x >= -w_half && x < w_half && y >= -h_half && y < h_half {
                let u_nearest = x.round() as i32;
                let v_nearest = y.round() as i32;
                
                let u_f = ((u_nearest % w as i32 + w as i32) % w as i32) as usize;
                let v_f = ((v_nearest % h as i32 + h as i32) % h as i32) as usize;
                
                dft.data[v_f * w + u_f]
            } else {
                Complex::default()
            };
            
            let idx = r_idx * angular_bins + theta_idx;
            magnitude[idx] = val.norm();
            phase[idx] = val.arg();
        }
    }
    
    PolarDftImage {
        width: w,
        height: h,
        radial_bins,
        angular_bins,
        magnitude,
        phase,
        cb: dft.cb.clone(),
        cr: dft.cr.clone(),
    }
}

/// Maps a Polar DFT representation back to Cartesian coordinates centered around the DC term.
/// Uses nearest-neighbor mapping to prevent interpolation-induced spectral attenuation.
pub fn polar_to_dft(polar: &PolarDftImage) -> DftImage {
    let w = polar.width;
    let h = polar.height;
    let r_max = ((w as f64 / 2.0).powi(2) + (h as f64 / 2.0).powi(2)).sqrt();
    
    let nr = polar.radial_bins;
    let n_theta = polar.angular_bins;
    let size = w * h;
    let mut data = vec![Complex::default(); size];
    
    for v in 0..h {
        let y = if v >= h / 2 { (v as f64) - (h as f64) } else { v as f64 };
        for u in 0..w {
            let x = if u >= w / 2 { (u as f64) - (w as f64) } else { u as f64 };
            
            let r = (x.powi(2) + y.powi(2)).sqrt();
            let mut theta = y.atan2(x);
            if theta < 0.0 {
                theta += 2.0 * std::f64::consts::PI;
            }
            
            // Map (r, theta) to polar grid coordinate indices
            let r_idx = r * ((nr - 1) as f64) / r_max;
            let theta_idx = theta * (n_theta as f64) / (2.0 * std::f64::consts::PI);
            
            let r_nearest = r_idx.round().clamp(0.0, (nr - 1) as f64) as usize;
            let t_nearest = ((theta_idx.round() as i32 % n_theta as i32 + n_theta as i32) % n_theta as i32) as usize;
            
            let idx = r_nearest * n_theta + t_nearest;
            data[v * w + u] = Complex::from_polar(polar.magnitude[idx], polar.phase[idx]);
        }
    }
    
    DftImage {
        width: w,
        height: h,
        data,
        cb: polar.cb.clone(),
        cr: polar.cr.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::YCbCrImage;
    use crate::dft::{compute_2d_dft, compute_2d_idft};

    #[test]
    fn test_polar_round_trip() {
        let w = 32;
        let h = 32;
        let size = w * h;
        
        let mut y = Vec::with_capacity(size);
        for i in 0..size {
            y.push(((i as f64 * 0.25).sin() * 50.0) + 128.0);
        }
        
        let original_ycbcr = YCbCrImage {
            width: w as u32,
            height: h as u32,
            y,
            cb: vec![128.0; size],
            cr: vec![128.0; size],
        };
        
        let dft = compute_2d_dft(&original_ycbcr);
        let polar = dft_to_polar(&dft);
        
        assert_eq!(polar.width, w);
        assert_eq!(polar.height, h);
        assert_eq!(polar.angular_bins, 360);
        
        let reconstructed_dft = polar_to_dft(&polar);
        let reconstructed_ycbcr = compute_2d_idft(&reconstructed_dft);
        
        // With nearest-neighbor, the values should match very closely
        for i in 0..size {
            let diff = (original_ycbcr.y[i] - reconstructed_ycbcr.y[i]).abs();
            assert!(diff < 15.0, "Mismatch at index {}, original={}, reconstructed={}, diff={}", i, original_ycbcr.y[i], reconstructed_ycbcr.y[i], diff);
        }
    }
}
