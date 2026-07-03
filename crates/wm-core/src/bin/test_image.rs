use std::path::Path;
use std::time::Instant;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use wm_core::{
    rgb_to_ycbcr, ycbcr_to_rgb, compute_grid, extract_tile_y_channel,
    write_tile_y_channel, compute_2d_dft, compute_2d_idft, dft_to_polar, polar_to_dft,
    compute_radial_profile, reconstruct_polar_magnitude, qim_embed, qim_decode_single,
    compute_phash256, hamming_distance, embed_grid_metadata, extract_grid_metadata,
    compute_spreading_seed, generate_payload, chacha20_select_coefficients,
    dct_8x8, idct_8x8, extract_8x8_block, write_8x8_block, zigzag_to_rowcol,
    get_perceptual_delta, compute_qim_offset, GridMetadata, TILE_SIZE
};

type HmacSha256 = Hmac<Sha256>;

fn hmac_sha256(key: &[u8], data: &[u8]) -> [u8; 32] {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC can take key of any size");
    mac.update(data);
    let result = mac.finalize();
    result.into_bytes().into()
}

/// Helper function to compute 64-bit coarse visual hash (simulating Layer 1 component)
pub fn compute_coarse_hash(image: &image::DynamicImage) -> u64 {
    let gray = image.to_luma8();
    // 8x8 nearest neighbor downsample
    let small = image::imageops::resize(&gray, 8, 8, image::imageops::FilterType::Nearest);
    let mut sum = 0.0;
    let mut pixels = vec![0.0; 64];
    for y in 0..8 {
        for x in 0..8 {
            let px = small.get_pixel(x, y)[0] as f64;
            pixels[(y * 8 + x) as usize] = px;
            sum += px;
        }
    }
    let mean = sum / 64.0;
    let mut hash = 0u64;
    for (i, &px) in pixels.iter().enumerate() {
        if px > mean {
            hash |= 1u64 << (63 - i);
        }
    }
    hash
}

/// Embeds the payload inside tiles using the block DCT-domain QIM.
pub fn embed_payload_in_tiles(
    ycbcr: &mut wm_core::YCbCrImage,
    grid: &wm_core::Grid,
    spreading_seed: &[u8; 32],
    payload: &[u8; 32],
    delta: f64,
) {
    for tile_idx in 0..grid.total_tiles {
        let tx = (tile_idx % grid.tiles_x) * TILE_SIZE;
        let ty = (tile_idx / grid.tiles_x) * TILE_SIZE;
        
        let mut tile_y = extract_tile_y_channel(ycbcr, tx, ty);
        
        for bit_idx in 0u16..256 {
            let bit_value = (payload[bit_idx as usize / 8] >> (7 - (bit_idx % 8))) & 1;
            
            // Derive per-tile-per-bit seed
            let seed_tb = hmac_sha256(
                spreading_seed,
                &[
                    (tile_idx as u32).to_le_bytes().as_slice(),
                    (bit_idx as u16).to_le_bytes().as_slice(),
                ].concat(),
            );
            
            // Select 20 coefficient locations via ChaCha20
            let coeff_indices = chacha20_select_coefficients(&seed_tb, 20, 5, 13);
            
            // Embed using QIM into each selected coefficient
            for &(block_row, block_col, coeff_zigzag) in &coeff_indices {
                let block = extract_8x8_block(&tile_y, block_row, block_col);
                let mut dct_block = dct_8x8(&block);
                let (r, c) = zigzag_to_rowcol(coeff_zigzag);
                dct_block[r][c] = qim_embed(dct_block[r][c], bit_value, delta, 0.0);
                let new_block = idct_8x8(&dct_block);
                write_8x8_block(&mut tile_y, block_row, block_col, &new_block);
            }
        }
        
        write_tile_y_channel(ycbcr, tx, ty, &tile_y);
    }
}

fn main() {
    let input_path = "/Users/ranikumari/.gemini/antigravity/brain/7469a49e-3cbf-4ed2-a8ec-ff9d096f2321/media__1783114100740.jpg";
    let output_path = "/Users/ranikumari/.gemini/antigravity/brain/7469a49e-3cbf-4ed2-a8ec-ff9d096f2321/watermarked_output.png";

    println!("--- AI Image Watermarking System End-to-End DSP Pipeline Test ---");
    println!("Loading input image: {}", input_path);

    if !Path::new(input_path).exists() {
        eprintln!("Error: Input image not found!");
        return;
    }

    let img = image::open(input_path).expect("Failed to open input image");
    let (width, height) = (img.width(), img.height());
    println!("Image Dimensions: {} x {}", width, height);

    // 1. Grid setup
    let grid = match compute_grid(width, height) {
        Some(g) => g,
        None => {
            eprintln!("Error: Image is too small for watermarking (must be at least 256x256)");
            return;
        }
    };
    println!("Grid Setup: {} x {} tiles (total tiles: {})", grid.tiles_x, grid.tiles_y, grid.total_tiles);

    // Compute original pHash
    let phash_start = Instant::now();
    let phash_original = compute_phash256(&img);
    let phash_duration = phash_start.elapsed();
    println!("Original pHash computed in: {:.2?}", phash_duration);

    // Mock device private and public keys
    let k_device_secret = [42u8; 32];
    let k_master_pub = [101u8; 32];

    // Derive payload and spreading seed
    let payload = generate_payload(&k_device_secret, &phash_original);
    let spreading_seed = compute_spreading_seed(&k_device_secret, &phash_original);

    // Convert whole image to YCbCr
    let color_start = Instant::now();
    let mut ycbcr = rgb_to_ycbcr(&img);
    let color_duration = color_start.elapsed();
    println!("RGB to YCbCr conversion: {:.2?}", color_duration);

    // 2. Global Radial DFT QIM embedding (Layer 1 signal)
    println!("\nApplying Layer 1 Global Radial DFT Watermark...");
    let dft = compute_2d_dft(&ycbcr);
    let mut polar = dft_to_polar(&dft);
    let old_profile = compute_radial_profile(&polar);
    let mut new_profile = old_profile.clone();
    let coarse_hash = compute_coarse_hash(&img);

    let f_low = 20usize;
    let f_high = f_low + 255; // 256 bins

    for f in f_low..=f_high {
        let bit_idx = f - f_low;
        let bit = (payload[bit_idx / 8] >> (7 - (bit_idx % 8))) & 1;
        let delta = get_perceptual_delta(f as u32);
        let offset = compute_qim_offset(&k_master_pub, coarse_hash, f as u32, delta);
        new_profile.values[f] = qim_embed(old_profile.values[f], bit, delta, offset);
    }

    let l1_start = Instant::now();
    reconstruct_polar_magnitude(&mut polar, &new_profile, &old_profile);
    let new_dft = polar_to_dft(&polar);
    ycbcr = compute_2d_idft(&new_dft);
    let l1_duration = l1_start.elapsed();
    println!("Layer 1 DFT Watermark embedding: {:.2?}", l1_duration);

    // 3. Per-tile DCT coefficient embedding (Layer 3 signal)
    println!("Applying Layer 3 Per-Tile Block DCT Watermark...");
    let delta_dct = 24.0; // QIM step for DCT
    let l3_start = Instant::now();
    embed_payload_in_tiles(&mut ycbcr, &grid, &spreading_seed, &payload, delta_dct);

    // 4. Convert back to RGB
    let wm_img = ycbcr_to_rgb(&ycbcr);
    let l3_duration = l3_start.elapsed();
    println!("Layer 3 DCT Watermark embedding + RGB conversion: {:.2?}", l3_duration);

    // Save watermarked image to disk
    wm_img.save(output_path).expect("Failed to save watermarked image");
    println!("Saved watermarked image to: {}", output_path);

    // 5. Embed metadata into the PNG file
    let mut positions = Vec::new();
    for ty in 0..grid.tiles_y {
        for tx in 0..grid.tiles_x {
            positions.push([tx * TILE_SIZE, ty * TILE_SIZE]);
        }
    }
    let grid_meta = GridMetadata {
        tiles_x: grid.tiles_x,
        tiles_y: grid.tiles_y,
        tile_size: TILE_SIZE,
        positions: positions.clone(),
    };
    let output_path_obj = Path::new(output_path);
    embed_grid_metadata(output_path_obj, &grid_meta).expect("Failed to embed grid metadata");
    println!("Successfully embedded grid metadata in image file!");

    // 6. Verify grid metadata extraction
    let ext_metadata = extract_grid_metadata(output_path_obj).expect("Failed to extract grid metadata");
    println!("Successfully verified grid metadata extraction from saved file! Total tiles: {}", ext_metadata.positions.len());

    // 7. Verification / Decoding of payload (Layer 3 simulation)
    println!("\nVerifying Layer 3 Payload Recovery from watermarked image pixels...");
    let decode_start = Instant::now();
    
    // We compute the approximate hash of the watermarked image and check the distance
    let phash_approx = compute_phash256(&wm_img);
    let phash_dist = hamming_distance(&phash_original, &phash_approx);
    println!("Hamming distance between original and watermarked full-image pHash: {} bits", phash_dist);

    // To test decoding logic correctness, we use the original spreading seed (simulating Layer 4 lookup)
    let spreading_seed_approx = spreading_seed;
    
    // Re-extract YCbCr to read decoded values
    let ext_ycbcr = rgb_to_ycbcr(&wm_img);
    let mut total_bits_checked = 0;
    let mut total_bit_errors = 0;

    for tile_idx in 0..grid.total_tiles {
        let [tx, ty] = ext_metadata.positions[tile_idx as usize];
        let tile_y = extract_tile_y_channel(&ext_ycbcr, tx, ty);
        
        let mut decoded_payload = [0u8; 32];
        for bit_idx in 0u16..256 {
            let seed_tb = hmac_sha256(
                &spreading_seed_approx,
                &[
                    (tile_idx as u32).to_le_bytes().as_slice(),
                    (bit_idx as u16).to_le_bytes().as_slice(),
                ].concat(),
            );
            let coeff_indices = chacha20_select_coefficients(&seed_tb, 20, 5, 13);
            
            let mut vote_0 = 0;
            let mut vote_1 = 0;
            for &(block_row, block_col, coeff_zigzag) in &coeff_indices {
                let block = extract_8x8_block(&tile_y, block_row, block_col);
                let dct_block = dct_8x8(&block);
                let (r, c) = zigzag_to_rowcol(coeff_zigzag);
                let bit = qim_decode_single(dct_block[r][c], delta_dct, 0.0);
                if bit == 0 {
                    vote_0 += 1;
                } else {
                    vote_1 += 1;
                }
            }
            let decoded_bit = if vote_0 > vote_1 { 0u8 } else { 1u8 };
            if decoded_bit == 1 {
                decoded_payload[bit_idx as usize / 8] |= 1 << (7 - (bit_idx % 8));
            }
        }

        // Calculate bit errors for this tile against original payload
        let mut tile_errors = 0;
        for b in 0..256 {
            let orig_bit = (payload[b / 8] >> (7 - (b % 8))) & 1;
            let dec_bit = (decoded_payload[b / 8] >> (7 - (b % 8))) & 1;
            if orig_bit != dec_bit {
                tile_errors += 1;
            }
        }
        total_bit_errors += tile_errors;
        total_bits_checked += 256;
        
        println!("  Tile {}: {}/256 bit errors | Accuracy: {:.1}%", 
                 tile_idx, tile_errors, (256 - tile_errors) as f64 / 256.0 * 100.0);
    }
    let decode_duration = decode_start.elapsed();
    println!("Layer 3 decoding over all tiles: {:.2?}", decode_duration);

    // 8. Calculate overall distortion metrics
    let mut sum_sq_diff = 0.0;
    let size = (ycbcr.width * ycbcr.height) as usize;
    let original_ycbcr = rgb_to_ycbcr(&img);
    for i in 0..size {
        let diff = original_ycbcr.y[i] - ycbcr.y[i];
        sum_sq_diff += diff * diff;
    }
    let mse = sum_sq_diff / (size as f64);
    let psnr = 20.0 * 255.0_f64.log10() - 10.0 * mse.log10();

    println!("\n--- Quality & Reliability Summary ---");
    println!("Total payload bit errors: {}/{} (Accuracy: {:.2}%)", 
             total_bit_errors, total_bits_checked, (total_bits_checked - total_bit_errors) as f64 / total_bits_checked as f64 * 100.0);
    println!("Mean Squared Error (MSE) of Y channel: {:.4}", mse);
    println!("Peak Signal-to-Noise Ratio (PSNR) of Y channel: {:.2} dB", psnr);
}
