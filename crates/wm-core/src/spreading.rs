use crate::phash::PHash256;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha20Rng;

type HmacSha256 = Hmac<Sha256>;

/// Helper to compute HMAC-SHA256 signature.
fn hmac_sha256(key: &[u8], data: &[u8]) -> [u8; 32] {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC can take key of any size");
    mac.update(data);
    let result = mac.finalize();
    result.into_bytes().into()
}

/// Computes the spreading seed: HMAC-SHA256(K_device, pHash_original)
pub fn compute_spreading_seed(k_device_secret: &[u8; 32], phash: &PHash256) -> [u8; 32] {
    hmac_sha256(k_device_secret, &phash.0)
}

/// Generates the 256-bit payload: HMAC-SHA256(K_device, "wm-payload-v1" || pHash_original)
pub fn generate_payload(k_device_secret: &[u8; 32], phash: &PHash256) -> [u8; 32] {
    let mut data = Vec::with_capacity(13 + 32);
    data.extend_from_slice(b"wm-payload-v1");
    data.extend_from_slice(&phash.0);
    hmac_sha256(k_device_secret, &data)
}

/// Uses ChaCha20 PRNG to select `n` unique coefficient indices from the specified zig-zag frequency band [band_lo, band_hi]
/// across all 32x32 blocks in a 256x256 tile.
/// Returns Vec<(block_row, block_col, coeff_zigzag_pos)>.
pub fn chacha20_select_coefficients(
    seed: &[u8; 32],
    n: usize,
    band_lo: usize,
    band_hi: usize,
) -> Vec<(u32, u32, usize)> {
    let mut rng = ChaCha20Rng::from_seed(*seed);
    let num_coeffs_per_block = band_hi - band_lo + 1; // 13 - 5 + 1 = 9
    let total_blocks = 32 * 32; // 1,024 blocks in a 256x256 tile
    let pool_size = total_blocks * num_coeffs_per_block; // 1,024 * 9 = 9,216
    
    let mut selected_indices = Vec::with_capacity(n);
    
    while selected_indices.len() < n {
        let candidate: usize = rng.gen_range(0..pool_size);
        if !selected_indices.contains(&candidate) {
            selected_indices.push(candidate);
        }
    }

    // Sort to ensure cache-friendly block-by-block processing
    selected_indices.sort_unstable();

    selected_indices
        .into_iter()
        .map(|idx| {
            let block_idx = idx / num_coeffs_per_block;
            let coeff_offset = idx % num_coeffs_per_block;
            
            let block_row = (block_idx / 32) as u32;
            let block_col = (block_idx % 32) as u32;
            let coeff_zigzag_pos = band_lo + coeff_offset;
            
            (block_row, block_col, coeff_zigzag_pos)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spreading_determinism() {
        let k_device = [42u8; 32];
        let phash = PHash256([7u8; 32]);

        let seed1 = compute_spreading_seed(&k_device, &phash);
        let seed2 = compute_spreading_seed(&k_device, &phash);
        assert_eq!(seed1, seed2);

        let payload1 = generate_payload(&k_device, &phash);
        let payload2 = generate_payload(&k_device, &phash);
        assert_eq!(payload1, payload2);
    }

    #[test]
    fn test_coefficient_selection() {
        let seed = [1u8; 32];
        let selections = chacha20_select_coefficients(&seed, 20, 5, 13);
        
        assert_eq!(selections.len(), 20);
        
        // Check uniqueness
        let mut unique_selections = selections.clone();
        unique_selections.dedup();
        assert_eq!(unique_selections.len(), 20);

        // Check range limits
        for (row, col, coeff) in selections {
            assert!(row < 32);
            assert!(col < 32);
            assert!(coeff >= 5 && coeff <= 13);
        }
    }
}
