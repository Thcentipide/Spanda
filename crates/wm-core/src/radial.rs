use crate::polar::PolarDftImage;

/// Represents a 1D radial energy profile of the DFT spectrum magnitude.
#[derive(Debug, Clone, PartialEq)]
pub struct RadialProfile {
    pub values: Vec<f64>, // Length: radial_bins
}

/// Integrates polar DFT magnitude across angular directions to obtain a 1D radial energy profile.
/// Implements: A(r) = (1 / N_theta) * sum_{theta=0}^{N_theta - 1} |F(r, theta)|
pub fn compute_radial_profile(polar: &PolarDftImage) -> RadialProfile {
    let mut values = vec![0.0; polar.radial_bins];
    for r in 0..polar.radial_bins {
        let mut sum = 0.0;
        for theta in 0..polar.angular_bins {
            sum += polar.magnitude[r * polar.angular_bins + theta];
        }
        values[r] = sum / (polar.angular_bins as f64);
    }
    RadialProfile { values }
}

/// Reconstructs the 2D polar magnitude spectrum from modified and original radial profiles.
/// Implements: |F_new(r, theta)| = |F(r, theta)| * (A'(r) / A(r))
pub fn reconstruct_polar_magnitude(
    polar: &mut PolarDftImage,
    new_profile: &RadialProfile,
    old_profile: &RadialProfile,
) {
    assert_eq!(polar.radial_bins, new_profile.values.len());
    assert_eq!(polar.radial_bins, old_profile.values.len());

    for r in 0..polar.radial_bins {
        let old_val = old_profile.values[r];
        let new_val = new_profile.values[r];
        
        let factor = if old_val > 1e-9 {
            new_val / old_val
        } else {
            1.0
        };

        for theta in 0..polar.angular_bins {
            let idx = r * polar.angular_bins + theta;
            polar.magnitude[idx] *= factor;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_radial_profile_and_reconstruction() {
        let mut polar = PolarDftImage {
            width: 16,
            height: 16,
            radial_bins: 5,
            angular_bins: 4,
            magnitude: vec![
                1.0, 2.0, 3.0, 4.0, // r = 0
                2.0, 2.0, 2.0, 2.0, // r = 1
                0.0, 0.0, 0.0, 0.0, // r = 2
                4.0, 8.0, 12.0, 16.0, // r = 3
                5.0, 5.0, 5.0, 5.0, // r = 4
            ],
            phase: vec![0.0; 20],
            cb: vec![],
            cr: vec![],
        };

        let old_profile = compute_radial_profile(&polar);
        assert_eq!(old_profile.values[0], 2.5); // (1+2+3+4)/4
        assert_eq!(old_profile.values[1], 2.0); // (2+2+2+2)/4
        assert_eq!(old_profile.values[2], 0.0); // 0.0
        assert_eq!(old_profile.values[3], 10.0); // (4+8+12+16)/4
        assert_eq!(old_profile.values[4], 5.0); // 5.0

        let mut new_profile = old_profile.clone();
        new_profile.values[0] = 5.0; // Double the magnitude
        new_profile.values[1] = 1.0; // Halve the magnitude
        new_profile.values[2] = 1.0; // Test division by zero fallback
        new_profile.values[3] = 10.0; // Same

        reconstruct_polar_magnitude(&mut polar, &new_profile, &old_profile);

        // Verify scaled magnitudes
        assert_eq!(polar.magnitude[0], 2.0);
        assert_eq!(polar.magnitude[1], 4.0);
        assert_eq!(polar.magnitude[4], 1.0);
        assert_eq!(polar.magnitude[8], 0.0); // Remains 0 due to 1e-9 check
        assert_eq!(polar.magnitude[12], 4.0);
    }
}
