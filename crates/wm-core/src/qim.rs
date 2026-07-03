use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

/// Helper to compute HMAC-SHA256 signature.
fn hmac_sha256(key: &[u8], data: &[u8]) -> [u8; 32] {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC can take key of any size");
    mac.update(data);
    let result = mac.finalize();
    result.into_bytes().into()
}

/// Embeds a payload bit into a value using Quantization Index Modulation (QIM).
/// For bit 0:
/// Q0 = delta * round((val - offset) / delta) + offset
/// For bit 1:
/// Q1 = delta * round((val - offset - delta/2) / delta) + offset + delta/2
pub fn qim_embed(val: f64, bit: u8, delta: f64, offset: f64) -> f64 {
    if bit == 0 {
        delta * ((val - offset) / delta).round() + offset
    } else {
        delta * ((val - offset - delta / 2.0) / delta).round() + offset + delta / 2.0
    }
}

/// Decodes a single value using QIM.
/// Returns 0 if value is closer to grid-0, 1 if closer to grid-1.
pub fn qim_decode_single(val: f64, delta: f64, offset: f64) -> u8 {
    let nearest_0 = delta * ((val - offset) / delta).round() + offset;
    let nearest_1 = delta * ((val - offset - delta / 2.0) / delta).round() + offset + delta / 2.0;
    
    let d0 = (val - nearest_0).abs();
    let d1 = (val - nearest_1).abs();
    
    if d0 < d1 { 0 } else { 1 }
}

/// Computes the per-frequency offset from K_master_pub and coarse hash.
pub fn compute_qim_offset(k_master_pub: &[u8; 32], coarse_hash: u64, f: u32, delta: f64) -> f64 {
    let mut data = Vec::with_capacity(8 + 4);
    data.extend_from_slice(&coarse_hash.to_le_bytes());
    data.extend_from_slice(&f.to_le_bytes());
    
    let seed = hmac_sha256(k_master_pub, &data);
    
    // Parse the first 8 bytes of the seed as a u64
    let mut val_bytes = [0u8; 8];
    val_bytes.copy_from_slice(&seed[0..8]);
    let val = u64::from_le_bytes(val_bytes);
    
    // Shifted pseudorandom offset in [0, delta)
    (val as f64) % delta
}

/// Returns the frequency-dependent QIM step delta(f) in [8.0, 32.0].
/// Smaller at low frequencies (preserves visual quality), larger at high frequencies (resists compression).
pub fn get_perceptual_delta(f: u32) -> f64 {
    let f_low = 10.0;
    let f_high = 200.0;
    let delta_min = 8.0;
    let delta_max = 32.0;

    let f_val = f as f64;
    if f_val <= f_low {
        delta_min
    } else if f_val >= f_high {
        delta_max
    } else {
        delta_min + (f_val - f_low) * (delta_max - delta_min) / (f_high - f_low)
    }
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

    #[test]
    fn test_qim_offset_determinism() {
        let key = [9u8; 32];
        let coarse_hash = 1234567890u64;
        let f = 100u32;
        let delta = 24.0;

        let o1 = compute_qim_offset(&key, coarse_hash, f, delta);
        let o2 = compute_qim_offset(&key, coarse_hash, f, delta);
        assert_eq!(o1, o2);
        assert!(o1 >= 0.0 && o1 < delta);
    }
}
