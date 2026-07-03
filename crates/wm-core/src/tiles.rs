use crate::color::YCbCrImage;
use image::DynamicImage;
use serde::{Serialize, Deserialize};
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

pub const TILE_SIZE: u32 = 256;

/// Represents the tile grid configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Grid {
    /// Number of tiles horizontally.
    pub tiles_x: u32,
    /// Number of tiles vertically.
    pub tiles_y: u32,
    /// Total number of tiles in the grid.
    pub total_tiles: u32,
}

/// Helper struct for image metadata storage and extraction.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct GridMetadata {
    pub tiles_x: u32,
    pub tiles_y: u32,
    pub tile_size: u32,
    pub positions: Vec<[u32; 2]>,
}

/// Computes the tile grid dimensions for an image.
/// The image must be at least 256x256 pixels.
pub fn compute_grid(width: u32, height: u32) -> Option<Grid> {
    if width < TILE_SIZE || height < TILE_SIZE {
        return None;
    }
    let tiles_x = width / TILE_SIZE;
    let tiles_y = height / TILE_SIZE;
    Some(Grid {
        tiles_x,
        tiles_y,
        total_tiles: tiles_x * tiles_y,
    })
}

/// Extracts a 256x256 pixel sub-image (tile) from a DynamicImage at the specified position.
pub fn extract_tile_at(image: &DynamicImage, pos: (u32, u32)) -> Option<DynamicImage> {
    let (x, y) = pos;
    if x + TILE_SIZE <= image.width() && y + TILE_SIZE <= image.height() {
        Some(image.crop_imm(x, y, TILE_SIZE, TILE_SIZE))
    } else {
        None
    }
}

/// Extracts a tile's Y (luminance) channel values from a YCbCrImage at the given pixel coordinates.
/// Returns a flat vector of size 256 * 256 = 65,536 in row-major order.
pub fn extract_tile_y_channel(image: &YCbCrImage, x: u32, y: u32) -> Vec<f64> {
    let mut tile = vec![0.0; (TILE_SIZE * TILE_SIZE) as usize];
    for dy in 0..TILE_SIZE {
        let y_coord = y + dy;
        let row_offset = (y_coord * image.width) as usize;
        let tile_offset = (dy * TILE_SIZE) as usize;
        for dx in 0..TILE_SIZE {
            let x_coord = x + dx;
            tile[tile_offset + dx as usize] = image.y[row_offset + x_coord as usize];
        }
    }
    tile
}

/// Writes a modified tile's Y channel values back into the source YCbCrImage buffer.
pub fn write_tile_y_channel(image: &mut YCbCrImage, x: u32, y: u32, tile: &[f64]) {
    for dy in 0..TILE_SIZE {
        let y_coord = y + dy;
        let row_offset = (y_coord * image.width) as usize;
        let tile_offset = (dy * TILE_SIZE) as usize;
        for dx in 0..TILE_SIZE {
            let x_coord = x + dx;
            image.y[row_offset + x_coord as usize] = tile[tile_offset + dx as usize];
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

/// Serializes and embeds the tile grid layout metadata into a PNG image file's custom text chunk.
pub fn embed_grid_metadata(file_path: &Path, grid_meta: &GridMetadata) -> Result<(), std::io::Error> {
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

    let json_str = serde_json::to_string(grid_meta).map_err(|e| {
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

/// Extracts the embedded grid layout metadata from a PNG image file's custom text chunk.
pub fn extract_grid_metadata(file_path: &Path) -> Option<GridMetadata> {
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
                    if let Ok(grid_meta) = serde_json::from_str::<GridMetadata>(json_str) {
                        return Some(grid_meta);
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
        assert_eq!(grid.total_tiles, 2);
    }

    #[test]
    fn test_tile_extraction_and_insertion() {
        let mut img = YCbCrImage {
            width: 512,
            height: 256,
            y: (0..512*256).map(|v| v as f64 + 1.0).collect(),
            cb: (0..512*256).map(|v| (v as f64 + 1.0) * 0.1).collect(),
            cr: (0..512*256).map(|v| (v as f64 + 1.0) * 0.2).collect(),
        };

        let tile = extract_tile_y_channel(&img, 256, 0);
        assert_eq!(tile.len(), 256 * 256);
        
        // Verify values are from the correct region
        assert_eq!(tile[0], img.y[256]);
        assert_eq!(tile[256], img.y[256 + 512]);

        // Modify tile Y channel
        let modified_tile = vec![0.0; 256 * 256];

        write_tile_y_channel(&mut img, 256, 0, &modified_tile);
        // Verify changes are written back to the correct region
        assert_eq!(img.y[256], 0.0);
        assert_eq!(img.y[256 + 512], 0.0);
        // Region 0 should remain unchanged
        assert_ne!(img.y[0], 0.0);
    }

    #[test]
    fn test_metadata_embedding_round_trip() {
        let grid_meta = GridMetadata {
            tiles_x: 2,
            tiles_y: 2,
            tile_size: 256,
            positions: vec![[0, 0], [256, 0], [0, 256], [256, 256]],
        };
        
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
        
        // Embed the grid metadata
        embed_grid_metadata(temp_path, &grid_meta).unwrap();
        
        // Extract the grid metadata
        let extracted_meta = extract_grid_metadata(temp_path).unwrap();
        assert_eq!(extracted_meta.tiles_x, grid_meta.tiles_x);
        assert_eq!(extracted_meta.tiles_y, grid_meta.tiles_y);
        assert_eq!(extracted_meta.positions.len(), grid_meta.positions.len());
        
        // Cleanup
        std::fs::remove_file(temp_path).ok();
    }
}
