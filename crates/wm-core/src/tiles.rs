use crate::color::YCbCrImage;
use serde::{Serialize, Deserialize};
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

/// Represents the physical coordinate metadata of a tile in the grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TilePosition {
    /// Column index of the tile (0-indexed).
    pub tile_x: u16,
    /// Row index of the tile (0-indexed).
    pub tile_y: u16,
    /// Pixel X start coordinate in the source image.
    pub x: u32,
    /// Pixel Y start coordinate in the source image.
    pub y: u32,
}

/// Represents the tile grid configuration and positions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImageGrid {
    /// Number of tiles horizontally.
    pub tiles_x: u16,
    /// Number of tiles vertically.
    pub tiles_y: u16,
    /// Metadata for each tile in the grid in row-major order.
    pub tile_positions: Vec<TilePosition>,
}

/// Computes the tile grid dimensions and metadata for an image.
/// The image must be at least 256x256 pixels.
pub fn compute_grid(width: u32, height: u32) -> Option<ImageGrid> {
    if width < 256 || height < 256 {
        return None;
    }

    let tiles_x = (width / 256) as u16;
    let tiles_y = (height / 256) as u16;
    let mut tile_positions = Vec::with_capacity((tiles_x * tiles_y) as usize);

    for ty in 0..tiles_y {
        for tx in 0..tiles_x {
            tile_positions.push(TilePosition {
                tile_x: tx,
                tile_y: ty,
                x: (tx as u32) * 256,
                y: (ty as u32) * 256,
            });
        }
    }

    Some(ImageGrid {
        tiles_x,
        tiles_y,
        tile_positions,
    })
}

/// Extracts a 256x256 pixel sub-image (tile) from a YCbCrImage at the specified position.
pub fn extract_tile(img: &YCbCrImage, pos: &TilePosition) -> YCbCrImage {
    let mut y = Vec::with_capacity(256 * 256);
    let mut cb = Vec::with_capacity(256 * 256);
    let mut cr = Vec::with_capacity(256 * 256);

    for dy in 0..256 {
        let y_coord = pos.y + dy;
        let row_offset = (y_coord * img.width) as usize;
        for dx in 0..256 {
            let x_coord = pos.x + dx;
            let idx = row_offset + (x_coord as usize);
            y.push(img.y[idx]);
            cb.push(img.cb[idx]);
            cr.push(img.cr[idx]);
        }
    }

    YCbCrImage {
        width: 256,
        height: 256,
        y,
        cb,
        cr,
    }
}

/// Writes a 256x256 processed tile back into the source YCbCrImage buffer.
pub fn insert_tile(img: &mut YCbCrImage, tile: &YCbCrImage, pos: &TilePosition) {
    assert_eq!(tile.width, 256);
    assert_eq!(tile.height, 256);

    for dy in 0..256 {
        let y_coord = pos.y + dy;
        let img_row_offset = (y_coord * img.width) as usize;
        let tile_row_offset = (dy * 256) as usize;
        for dx in 0..256 {
            let x_coord = pos.x + dx;
            let img_idx = img_row_offset + (x_coord as usize);
            let tile_idx = tile_row_offset + (dx as usize);
            img.y[img_idx] = tile.y[tile_idx];
            img.cb[img_idx] = tile.cb[tile_idx];
            img.cr[img_idx] = tile.cr[tile_idx];
        }
    }
}

/// Computes standard IEEE 802.3 CRC-32 checksum (polynomial 0xEDB88320).
fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFFFFFFu32;
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ 0xEDB88320;
            } else {
                crc >>= 1;
            }
        }
    }
    !crc
}

/// Serializes and embeds the tile grid layout into a PNG image file's custom text chunk.
pub fn embed_grid_metadata(file_path: &Path, grid: &ImageGrid) -> Result<(), std::io::Error> {
    let mut file = File::open(file_path)?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;

    // Check PNG signature: [137, 80, 78, 71, 13, 10, 26, 10]
    if bytes.len() < 8 || &bytes[0..8] != &[137, 80, 78, 71, 13, 10, 26, 10] {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Target file is not a valid PNG image",
        ));
    }

    let json_str = serde_json::to_string(grid).map_err(|e| {
        std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string())
    })?;

    // Create the tEXt chunk data:
    // Key: "wm_grid_metadata" (16 bytes) + Separator (0x00) + Value (JSON bytes)
    let mut chunk_data = Vec::new();
    chunk_data.extend_from_slice(b"wm_grid_metadata\0");
    chunk_data.extend_from_slice(json_str.as_bytes());

    let chunk_len = chunk_data.len() as u32;
    let chunk_type = b"tEXt";

    let mut crc_payload = Vec::with_capacity(4 + chunk_data.len());
    crc_payload.extend_from_slice(chunk_type);
    crc_payload.extend_from_slice(&chunk_data);
    let crc_val = crc32(&crc_payload);

    let mut new_bytes = Vec::new();
    new_bytes.extend_from_slice(&bytes[0..8]); // PNG signature

    let mut cursor = 8;
    let mut ihdr_written = false;

    while cursor < bytes.len() {
        if cursor + 8 > bytes.len() {
            break;
        }
        let len = u32::from_be_bytes([
            bytes[cursor],
            bytes[cursor + 1],
            bytes[cursor + 2],
            bytes[cursor + 3],
        ]) as usize;
        
        let chunk_t = &bytes[cursor + 4..cursor + 8];
        let total_chunk_len = 12 + len; // 4 (len) + 4 (type) + len (data) + 4 (crc)

        if cursor + total_chunk_len > bytes.len() {
            break;
        }

        // Write the existing chunk
        new_bytes.extend_from_slice(&bytes[cursor..cursor + total_chunk_len]);

        // If this chunk is IHDR, immediately write our custom tEXt chunk
        if chunk_t == b"IHDR" && !ihdr_written {
            // Write length
            new_bytes.extend_from_slice(&chunk_len.to_be_bytes());
            // Write type
            new_bytes.extend_from_slice(chunk_type);
            // Write data
            new_bytes.extend_from_slice(&chunk_data);
            // Write CRC
            new_bytes.extend_from_slice(&crc_val.to_be_bytes());
            ihdr_written = true;
        }

        cursor += total_chunk_len;
    }

    let mut out_file = File::create(file_path)?;
    out_file.write_all(&new_bytes)?;
    Ok(())
}

/// Extracts the embedded grid layout from a PNG image file's custom text chunk.
pub fn extract_grid_metadata(file_path: &Path) -> Option<ImageGrid> {
    let mut file = File::open(file_path).ok()?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).ok()?;

    if bytes.len() < 8 || &bytes[0..8] != &[137, 80, 78, 71, 13, 10, 26, 10] {
        return None;
    }

    let mut cursor = 8;
    while cursor < bytes.len() {
        if cursor + 8 > bytes.len() {
            break;
        }
        let len = u32::from_be_bytes([
            bytes[cursor],
            bytes[cursor + 1],
            bytes[cursor + 2],
            bytes[cursor + 3],
        ]) as usize;
        let chunk_t = &bytes[cursor + 4..cursor + 8];
        let total_chunk_len = 12 + len;

        if cursor + total_chunk_len > bytes.len() {
            break;
        }

        if chunk_t == b"tEXt" {
            let data = &bytes[cursor + 8..cursor + 8 + len];
            if data.starts_with(b"wm_grid_metadata\0") {
                let json_bytes = &data[17..]; // Skip "wm_grid_metadata\0" (17 bytes)
                if let Ok(json_str) = std::str::from_utf8(json_bytes) {
                    if let Ok(grid) = serde_json::from_str::<ImageGrid>(json_str) {
                        return Some(grid);
                    }
                }
            }
        }

        cursor += total_chunk_len;
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_grid_calculation() {
        assert_eq!(compute_grid(200, 200), None);
        assert_eq!(compute_grid(256, 255), None);

        let grid = compute_grid(512, 300).unwrap();
        assert_eq!(grid.tiles_x, 2);
        assert_eq!(grid.tiles_y, 1);
        assert_eq!(grid.tile_positions.len(), 2);
        assert_eq!(grid.tile_positions[0], TilePosition { tile_x: 0, tile_y: 0, x: 0, y: 0 });
        assert_eq!(grid.tile_positions[1], TilePosition { tile_x: 1, tile_y: 0, x: 256, y: 0 });
    }

    #[test]
    fn test_tile_extraction_and_insertion() {
        let mut img = YCbCrImage {
            width: 512,
            height: 256,
            y: (0..512*256).map(|v| v as f64 + 1.0).collect(), // Add 1.0 to avoid 0.0 at index 0
            cb: (0..512*256).map(|v| (v as f64 + 1.0) * 0.1).collect(),
            cr: (0..512*256).map(|v| (v as f64 + 1.0) * 0.2).collect(),
        };

        let grid = compute_grid(img.width, img.height).unwrap();
        let pos1 = grid.tile_positions[1]; // x = 256, y = 0
        
        let tile = extract_tile(&img, &pos1);
        assert_eq!(tile.width, 256);
        assert_eq!(tile.height, 256);
        
        // Verify values are from the correct region
        assert_eq!(tile.y[0], img.y[256]);
        assert_eq!(tile.y[256], img.y[256 + 512]);

        // Modify tile Y channel
        let mut modified_tile = tile;
        modified_tile.y = vec![0.0; 256 * 256];

        insert_tile(&mut img, &modified_tile, &pos1);
        // Verify changes are written back to the correct region
        assert_eq!(img.y[256], 0.0);
        assert_eq!(img.y[256 + 512], 0.0);
        // Region 0 should remain unchanged
        assert_ne!(img.y[0], 0.0);
    }

    #[test]
    fn test_metadata_embedding_round_trip() {
        // Create a dummy grid
        let grid = compute_grid(512, 512).unwrap();
        
        // Create a minimal valid 1x1 PNG file in memory to test embedding
        // PNG Signature (8 bytes) + IHDR Chunk (25 bytes) + IEND Chunk (12 bytes)
        let dummy_png_bytes: [u8; 45] = [
            137, 80, 78, 71, 13, 10, 26, 10, // signature
            0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 2, 0, 0, 0, 144, 110, 255, 63, // IHDR
            0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130 // IEND
        ];
        
        let temp_path = Path::new("/tmp/test_metadata_embed.png");
        {
            let mut file = File::create(temp_path).unwrap();
            file.write_all(&dummy_png_bytes).unwrap();
        }
        
        // Embed the grid
        embed_grid_metadata(temp_path, &grid).unwrap();
        
        // Extract the grid
        let extracted_grid = extract_grid_metadata(temp_path).unwrap();
        assert_eq!(extracted_grid.tiles_x, grid.tiles_x);
        assert_eq!(extracted_grid.tiles_y, grid.tiles_y);
        assert_eq!(extracted_grid.tile_positions.len(), grid.tile_positions.len());
        
        // Cleanup
        std::fs::remove_file(temp_path).ok();
    }
}
