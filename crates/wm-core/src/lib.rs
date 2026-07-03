pub mod color;
pub mod phash;
pub mod dft;
pub mod polar;
pub mod radial;
pub mod qim;
pub mod tiles;
pub mod spreading;
pub mod dct;

pub use color::{rgb_to_ycbcr, ycbcr_to_rgb, YCbCrImage};
pub use phash::{compute_phash256, PHash256, hamming_distance};
pub use dft::{compute_2d_dft, compute_2d_idft, DftImage};
pub use polar::{dft_to_polar, polar_to_dft, PolarDftImage};
pub use radial::{compute_radial_profile, reconstruct_polar_magnitude, RadialProfile};
pub use qim::{qim_embed, qim_decode_single, compute_qim_offset, get_perceptual_delta};
pub use tiles::{
    compute_grid, extract_tile_at, extract_tile_y_channel, write_tile_y_channel,
    Grid, GridMetadata, embed_grid_metadata, extract_grid_metadata, TILE_SIZE
};
pub use spreading::{compute_spreading_seed, generate_payload, chacha20_select_coefficients};
pub use dct::{
    dct_8x8, idct_8x8, dct_2d_32x32, extract_8x8_block, write_8x8_block, zigzag_to_rowcol
};
