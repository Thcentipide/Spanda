use std::path::Path;
use wm_core::{
    rgb_to_ycbcr, ycbcr_to_rgb, compute_grid, extract_tile, insert_tile,
    compute_2d_dft, compute_2d_idft, dft_to_polar, polar_to_dft,
    compute_radial_profile, reconstruct_polar_magnitude, qim_embed, qim_decode_single,
    compute_phash256, embed_grid_metadata, extract_grid_metadata
};

fn main() {
    let input_path = "/Users/ranikumari/.gemini/antigravity/brain/7469a49e-3cbf-4ed2-a8ec-ff9d096f2321/media__1782929863927.jpg";
    let output_path = "/Users/ranikumari/.gemini/antigravity/brain/7469a49e-3cbf-4ed2-a8ec-ff9d096f2321/watermarked_output.png";

    println!("--- AI Image Watermarking System DSP Pipeline Test ---");
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
    println!("Grid Setup: {} x {} tiles (total tiles: {})", grid.tiles_x, grid.tiles_y, grid.tile_positions.len());

    // Convert whole image to YCbCr
    let original_ycbcr = rgb_to_ycbcr(&img);
    let mut watermarked_ycbcr = original_ycbcr.clone();

    // 2. Define the payload
    // Generate a 256-bit mock payload (alternating 0s and 1s)
    let mut payload = [0u8; 256];
    for i in 0..256 {
        payload[i] = (i % 2) as u8;
    }

    let delta = 20.0; // Quantization step
    let offset = 0.0; // Offset function (placeholder)
    let f_low = 20;  // Start of middle frequency band

    println!("\nEmbedding watermark in tiles...");
    let mut total_bit_errors = 0;
    
    for (idx, pos) in grid.tile_positions.iter().enumerate() {
        println!("  Processing Tile {} at (col: {}, row: {})", idx, pos.tile_x, pos.tile_y);

        // A. Extract tile YCbCr
        let tile_ycbcr = extract_tile(&original_ycbcr, pos);

        // B. Calculate original tile pHash
        let tile_rgb = ycbcr_to_rgb(&tile_ycbcr);
        let orig_hash = compute_phash256(&tile_rgb);

        // C. Forward 2D DFT on Y channel
        let dft = compute_2d_dft(&tile_ycbcr);

        // D. Convert spectrum to Polar coordinates
        let mut polar = dft_to_polar(&dft);

        // E. Compute radial profile
        let old_profile = compute_radial_profile(&polar);
        let mut new_profile = old_profile.clone();

        // F. Quantize target band coefficients to embed payload (64 bits, spaced by 2 bins to avoid crosstalk)
        for f in 0..64 {
            let bit = payload[f];
            let freq = f_low + f * 2;
            new_profile.values[freq] = qim_embed(old_profile.values[freq], bit, delta, offset);
        }

        // G. Reconstruct polar magnitude
        reconstruct_polar_magnitude(&mut polar, &new_profile, &old_profile);

        // H. Map polar back to Cartesian DFT
        let new_dft = polar_to_dft(&polar);

        // I. Inverse 2D DFT
        let watermarked_tile_ycbcr = compute_2d_idft(&new_dft);

        // J. Stitch processed tile back into the watermarked image buffer
        insert_tile(&mut watermarked_ycbcr, &watermarked_tile_ycbcr, pos);

        // --- VERIFY TILE DECODING ---
        // Extract tile back to verify payload recovery
        let extracted_tile = extract_tile(&watermarked_ycbcr, pos);
        let ext_dft = compute_2d_dft(&extracted_tile);
        let ext_polar = dft_to_polar(&ext_dft);
        let ext_profile = compute_radial_profile(&ext_polar);

        let mut tile_errors = 0;
        for f in 0..64 {
            let freq = f_low + f * 2;
            let decoded_bit = qim_decode_single(ext_profile.values[freq], delta, offset);
            if decoded_bit != payload[f] {
                tile_errors += 1;
            }
        }
        total_bit_errors += tile_errors;

        if idx == 0 {
            println!("      Sample Values (Tile 0):");
            for f in 0..5 {
                let freq = f_low + f * 2;
                let orig_val = old_profile.values[freq];
                let emb_val = new_profile.values[freq];
                let ext_val = ext_profile.values[freq];
                let dec_bit = qim_decode_single(ext_val, delta, offset);
                println!("        Freq {}: orig={:.4}, embedded={:.4}, extracted={:.4}, target_bit={}, decoded={}", freq, orig_val, emb_val, ext_val, payload[f], dec_bit);
            }
        }

        // Calculate post-watermarked pHash and Hamming distance
        let ext_rgb = ycbcr_to_rgb(&extracted_tile);
        let new_hash = compute_phash256(&ext_rgb);
        let mut hamming_dist = 0;
        for b in 0..32 {
            hamming_dist += (orig_hash.0[b] ^ new_hash.0[b]).count_ones();
        }

        println!("    -> Bit errors: {}/256 | Hamming distance of pHash: {} bits", tile_errors, hamming_dist);
    }

    // 3. Save watermarked image
    let watermarked_img = ycbcr_to_rgb(&watermarked_ycbcr);
    watermarked_img.save(output_path).expect("Failed to save watermarked image");
    println!("\nSaved watermarked image to: {}", output_path);

    // Embed metadata
    let output_path_obj = Path::new(output_path);
    embed_grid_metadata(output_path_obj, &grid).expect("Failed to embed grid metadata");
    println!("Successfully embedded grid metadata in image file!");

    // Verify extraction
    let ext_metadata = extract_grid_metadata(output_path_obj).expect("Failed to extract grid metadata");
    println!("Successfully verified grid metadata extraction from saved file! Grid dimensions read: {}x{} (total tiles: {})", 
             ext_metadata.tiles_x, ext_metadata.tiles_y, ext_metadata.tile_positions.len());

    // 4. Calculate overall distortion metrics
    let mut sum_sq_diff = 0.0;
    let size = (original_ycbcr.width * original_ycbcr.height) as usize;
    for i in 0..size {
        let diff = original_ycbcr.y[i] - watermarked_ycbcr.y[i];
        sum_sq_diff += diff * diff;
    }
    let mse = sum_sq_diff / (size as f64);
    let psnr = 20.0 * 255.0_f64.log10() - 10.0 * mse.log10();

    println!("\n--- Quality & Reliability Summary ---");
    println!("Total payload bit errors across all tiles: {}/{}", total_bit_errors, grid.tile_positions.len() * 64);
    println!("Mean Squared Error (MSE) of Y channel: {:.4}", mse);
    println!("Peak Signal-to-Noise Ratio (PSNR) of Y channel: {:.2} dB", psnr);
    
    if psnr >= 40.0 {
        println!("Watermark visibility: IMPERCEPTIBLE (Excellent)");
    } else if psnr >= 35.0 {
        println!("Watermark visibility: SUBTLE (Good)");
    } else {
        println!("Watermark visibility: NOTICEABLE (Needs delta calibration)");
    }
}
