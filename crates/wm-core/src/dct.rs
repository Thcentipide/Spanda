/// Computes the 2D DCT-II of an 8x8 block of pixel values.
pub fn dct_8x8(block: &[[f64; 8]; 8]) -> [[f64; 8]; 8] {
    let mut dct = [[0.0; 8]; 8];
    
    // Precompute cosine values
    let mut cos_table = [[0.0; 8]; 8];
    for u in 0..8 {
        for x in 0..8 {
            cos_table[u][x] = (((2 * x + 1) as f64 * u as f64 * std::f64::consts::PI) / 16.0).cos();
        }
    }

    for u in 0..8 {
        let cu = if u == 0 { 1.0 / 2.0_f64.sqrt() } else { 1.0 };
        for v in 0..8 {
            let cv = if v == 0 { 1.0 / 2.0_f64.sqrt() } else { 1.0 };
            
            let mut sum = 0.0;
            for x in 0..8 {
                for y in 0..8 {
                    sum += block[x][y] * cos_table[u][x] * cos_table[v][y];
                }
            }
            dct[u][v] = 0.25 * cu * cv * sum;
        }
    }
    dct
}

/// Computes the 2D IDCT-III of an 8x8 block of coefficients.
pub fn idct_8x8(dct_block: &[[f64; 8]; 8]) -> [[f64; 8]; 8] {
    let mut block = [[0.0; 8]; 8];
    
    // Precompute cosine values
    let mut cos_table = [[0.0; 8]; 8];
    for u in 0..8 {
        for x in 0..8 {
            cos_table[u][x] = (((2 * x + 1) as f64 * u as f64 * std::f64::consts::PI) / 16.0).cos();
        }
    }

    for x in 0..8 {
        for y in 0..8 {
            let mut sum = 0.0;
            for u in 0..8 {
                let cu = if u == 0 { 1.0 / 2.0_f64.sqrt() } else { 1.0 };
                for v in 0..8 {
                    let cv = if v == 0 { 1.0 / 2.0_f64.sqrt() } else { 1.0 };
                    sum += cu * cv * dct_block[u][v] * cos_table[u][x] * cos_table[v][y];
                }
            }
            block[x][y] = 0.25 * sum;
        }
    }
    block
}

/// Computes the 2D DCT-II of a 32x32 matrix of values (used for pHash).
pub fn dct_2d_32x32(pixels: &[[f64; 32]; 32]) -> [[f64; 32]; 32] {
    let mut dct = [[0.0; 32]; 32];
    
    // Precompute cosine values
    let mut cos_table = [[0.0; 32]; 32];
    for u in 0..32 {
        for x in 0..32 {
            cos_table[u][x] = (((2 * x + 1) as f64 * u as f64 * std::f64::consts::PI) / 64.0).cos();
        }
    }

    for u in 0..32 {
        let cu = if u == 0 { 1.0 / 2.0_f64.sqrt() } else { 1.0 };
        for v in 0..32 {
            let cv = if v == 0 { 1.0 / 2.0_f64.sqrt() } else { 1.0 };
            
            let mut sum = 0.0;
            for x in 0..32 {
                for y in 0..32 {
                    sum += pixels[x][y] * cos_table[u][x] * cos_table[v][y];
                }
            }
            dct[u][v] = 0.25 * cu * cv * sum;
        }
    }
    dct
}

/// Extracts an 8x8 pixel block from a flat 256x256 tile's channel data.
pub fn extract_8x8_block(tile: &[f64], block_row: u32, block_col: u32) -> [[f64; 8]; 8] {
    let mut block = [[0.0; 8]; 8];
    let start_y = (block_row * 8) as usize;
    let start_x = (block_col * 8) as usize;
    
    for r in 0..8 {
        let row_offset = (start_y + r) * 256;
        for c in 0..8 {
            block[r][c] = tile[row_offset + start_x + c];
        }
    }
    block
}

/// Writes an 8x8 block of pixel values back into a flat 256x256 tile's channel data.
pub fn write_8x8_block(tile: &mut [f64], block_row: u32, block_col: u32, block: &[[f64; 8]; 8]) {
    let start_y = (block_row * 8) as usize;
    let start_x = (block_col * 8) as usize;
    
    for r in 0..8 {
        let row_offset = (start_y + r) * 256;
        for c in 0..8 {
            tile[row_offset + start_x + c] = block[r][c];
        }
    }
}

/// Zig-zag ordering table to map 1D index (0..63) to (row, col) coordinates inside an 8x8 block.
const ZIGZAG_TABLE: [(usize, usize); 64] = [
    (0, 0),
    (0, 1), (1, 0),
    (2, 0), (1, 1), (0, 2),
    (0, 3), (1, 2), (2, 1), (3, 0),
    (4, 0), (3, 1), (2, 2), (1, 3), (0, 4),
    (0, 5), (1, 4), (2, 3), (3, 2), (4, 1), (5, 0),
    (6, 0), (5, 1), (4, 2), (3, 3), (2, 4), (1, 5), (0, 6),
    (0, 7), (1, 6), (2, 5), (3, 4), (4, 3), (5, 2), (6, 1), (7, 0),
    (7, 1), (6, 2), (5, 3), (4, 4), (3, 5), (2, 6), (1, 7),
    (2, 7), (3, 6), (4, 5), (5, 4), (6, 3), (7, 2),
    (7, 3), (6, 4), (5, 5), (4, 6), (3, 7),
    (4, 7), (5, 6), (6, 5), (7, 4),
    (7, 5), (6, 6), (5, 7),
    (6, 7), (7, 6),
    (7, 7)
];

/// Converts zig-zag index (0..63) to (row, col) inside an 8x8 block.
pub fn zigzag_to_rowcol(zigzag_idx: usize) -> (usize, usize) {
    ZIGZAG_TABLE[zigzag_idx]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dct_8x8_round_trip() {
        let original = [
            [120.0, 125.0, 122.0, 118.0, 115.0, 110.0, 105.0, 100.0],
            [122.0, 123.0, 124.0, 120.0, 118.0, 112.0, 108.0, 102.0],
            [125.0, 126.0, 125.0, 121.0, 120.0, 115.0, 110.0, 105.0],
            [120.0, 122.0, 121.0, 119.0, 117.0, 113.0, 107.0, 103.0],
            [115.0, 117.0, 116.0, 115.0, 112.0, 110.0, 104.0, 100.0],
            [110.0, 112.0, 111.0, 109.0, 108.0, 105.0, 101.0, 98.0],
            [105.0, 107.0, 106.0, 104.0, 103.0, 101.0, 97.0, 95.0],
            [100.0, 102.0, 101.0, 99.0, 98.0, 96.0, 94.0, 92.0],
        ];

        let coeffs = dct_8x8(&original);
        let reconstructed = idct_8x8(&coeffs);

        for x in 0..8 {
            for y in 0..8 {
                let diff = (original[x][y] - reconstructed[x][y]).abs();
                assert!(diff < 1e-9, "Mismatch at ({}, {}): diff = {}", x, y, diff);
            }
        }
    }

    #[test]
    fn test_zigzag_mapping() {
        assert_eq!(zigzag_to_rowcol(0), (0, 0));
        assert_eq!(zigzag_to_rowcol(1), (0, 1));
        assert_eq!(zigzag_to_rowcol(2), (1, 0));
        assert_eq!(zigzag_to_rowcol(63), (7, 7));
    }
}
