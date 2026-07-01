/// Embeds a payload bit into a single value using Quantization Index Modulation (QIM).
/// Formula: x' = round((x - o - b * delta) / (2 * delta)) * (2 * delta) + o + b * delta
pub fn qim_embed(val: f64, bit: u8, delta: f64, offset: f64) -> f64 {
    let b = bit as f64;
    let shifted = val - offset - b * delta;
    let k = (shifted / (2.0 * delta)).round();
    k * 2.0 * delta + offset + b * delta
}

/// Decodes a single value using QIM.
/// Formula: b = round((x' - o) / delta) % 2
pub fn qim_decode_single(val: f64, delta: f64, offset: f64) -> u8 {
    let shifted = val - offset;
    let k = (shifted / delta).round() as i64;
    // Safely handle negative modulo in Rust
    ((k % 2).abs()) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_qim_round_trip() {
        let delta = 16.0;
        let offset = 4.32;
        
        let test_vals = vec![-150.0, -22.5, 0.0, 1.25, 78.9, 500.0];
        for val in test_vals {
            for bit in vec![0, 1] {
                let embedded = qim_embed(val, bit, delta, offset);
                let decoded = qim_decode_single(embedded, delta, offset);
                assert_eq!(decoded, bit, "Failed for val={}, bit={}", val, bit);
            }
        }
    }

    #[test]
    fn test_qim_noise_robustness() {
        let delta = 20.0;
        let offset = 5.0;
        let original_val = 120.0;
        
        for bit in vec![0, 1] {
            let embedded = qim_embed(original_val, bit, delta, offset);
            
            // Noise within (-delta/4, delta/4) should not affect the decoded bit
            let max_noise = delta / 4.0 - 0.01;
            let noises = vec![-max_noise, -2.0, 0.0, 2.0, max_noise];
            
            for noise in noises {
                let noisy_embedded = embedded + noise;
                let decoded = qim_decode_single(noisy_embedded, delta, offset);
                assert_eq!(decoded, bit, "Failed under noise {} for bit {}", noise, bit);
            }
        }
    }
}
