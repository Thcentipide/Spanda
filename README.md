# Spanda — AI Image Watermarking System

An open-source Rust system for embedding cryptographically verifiable watermarks into AI-generated images. The watermark is a permanent, measurable modification to the frequency-domain content of the image — not metadata, not steganographic bit-flipping, not an invisible overlay.

Verification requires only the image itself, a single permanent public key ($K_{master\_pub}$), and access to a distributed public ledger. No central server is contacted at any point during verification.

---

## Table of Contents

- [Why This Exists](#why-this-exists)
- [How It Differs from SynthID](#how-it-differs-from-synthid)
- [Architecture](#architecture)
  - [Key Hierarchy](#key-hierarchy)
  - [Embedding Pipeline](#embedding-pipeline)
  - [Verification Pipeline](#verification-pipeline)
- [Watermark Embedding](#watermark-embedding)
  - [Global Radial DFT Modulation (Layer 1 Signal)](#global-radial-dft-modulation-layer-1-signal)
  - [Per-Tile DCT Coefficient Embedding (Layer 3 Signal)](#per-tile-dct-coefficient-embedding-layer-3-signal)
  - [Perceptual Hashing (pHash256)](#perceptual-hashing-phash256)
- [Detection and Verification](#detection-and-verification)
  - [Layer 1: Physical Presence Detection](#layer-1-physical-presence-detection)
  - [Layer 2: Ledger Authentication](#layer-2-ledger-authentication)
  - [Layer 3: Device Key Extraction](#layer-3-device-key-extraction)
  - [Layer 4: Authority Extraction](#layer-4-authority-extraction)
- [Ledger System](#ledger-system)
  - [Entry Structure](#entry-structure)
  - [Multi-Index Hashing (MIH)](#multi-index-hashing-mih)
  - [Merkle Tree](#merkle-tree)
- [Security Model](#security-model)
- [Robustness](#robustness)
- [Crate Structure](#crate-structure)
- [Build and Run](#build-and-run)
- [Full Specification](#full-specification)

---

## Why This Exists

Current AI watermarking systems rely on security through obscurity — fixed carrier frequencies, shared embedding patterns, and closed-source detection. These collapse under statistical analysis. An attacker who collects enough watermarked images can average out the consistent signal and subtract it.

This system treats the problem differently. Security derives from cryptographic mathematics, not access restrictions. The watermark location is unique per image and per device, derived via HMAC from a secret device key and the image's own perceptual hash. There is no consistent signal across images to average.

The system is open source by design. Robustness is proven by surviving public scrutiny, not by hiding the algorithm.

---

## How It Differs from SynthID

Google's SynthID embedded watermarks at fixed carrier frequencies and phases shared across all images from a given model. An attacker collected approximately 200,000 watermarked images, averaged the frequency-domain signatures, and subtracted the result — achieving 91% removal effectiveness while maintaining PSNR above 35 dB.

This system defeats statistical averaging because:

$$\text{spreading\_seed} = \text{HMAC-SHA256}(K_{device\_secret},\ \text{pHash}(I))$$

The spreading seed is unique per image and per device. No two watermarks share the same DCT coefficient locations. Even if the same device watermarks two visually similar images, the pHash difference produces a completely different HMAC output and therefore a completely different set of modified coefficients.

---

## Architecture

### Key Hierarchy

```
K_master_secret  ──HKDF──>  K_device_secret  ──HMAC──>  spreading_seed
      |                          |                             |
      |                     (Ed25519)                   (per image,
      v                          |                       per device)
K_master_pub                K_device_pub                     |
(permanent,                (in certificate)           ──ChaCha20──> coefficient indices
 baked into verifier)
```

- **K_master_secret / K_master_pub**: A single Ed25519 keypair for the entire system's lifetime. The private key lives exclusively on the key derivation server (HSM or equivalent). The public key is baked into every verifier binary at compile time. Never rotated, never versioned.
- **K_device_secret / K_device_pub**: Per-device Ed25519 keypairs derived from the master secret via HKDF-SHA256 with a per-device salt (`device_id || nonce`) and info string `"wm-device-key-v1"`. The device receives its secret key and a certificate signed by the master key during provisioning.
- **AuthorityCertificate**: Binds a device's public key to an organization name (e.g. "Google", "Apple", "Stability AI"), signed by `K_master_secret`. Contains `device_pub`, `org_name`, `device_id`, `expiry_ms`, `nonce`, and the master signature. Verified against `K_master_pub` by any ledger node or verifier.

> [!IMPORTANT]
> **Key Governance & Operational Security:**
> The `K_master_secret` is the ultimate secret in the system. Its use for re-deriving device keys and performing Layer 4 authority extraction is restricted to **extremely unusual, final cases** (such as official court orders, subpoenas, or critical legal requests). Under all normal operations, verification is conducted purely offline using the public key and transparency ledger without involving any master keys.

### Embedding Pipeline

```
AI Model --> Raw Image (I)
                |
                +--> compute_phash256(I) --> pHash_original
                |                              |
                |                  +-----------+
                |                  |           |
                |          payload = HMAC(K_dev, "wm-payload-v1" || pH)
                |          spreading_seed = HMAC(K_dev, pH)
                |                  |
                v                  v
          [Global DFT Radial QIM Modulation]
                |
                v
          [Per-Tile DCT Coefficient Embedding]
                |
                v
          Watermarked Image (I')
                |
                +--> compute_phash256(I') --> pHash_watermarked
                +--> extract tiles, compute per-tile pHashes
                +--> write grid metadata to PNG tEXt / JPEG COM
                |
                v
          [Construct LedgerEntry, sign with K_device_secret]
                |
                v
          Submit to Public Ledger (append-only Merkle tree)
```

### Verification Pipeline

Verification follows a 4-layer architecture. Layers 1 and 2 are public and require no private keys. Layers 3 and 4 require the device private key and master private key respectively.

```
Suspect Image --> [Layer 1: DFT Radial Projection Test]
                       |
                       v
                  Watermark Presence: YES / NO
                       |
                       v
                  [Layer 2: Tile pHash -> Ledger Query]
                       |
                       +-- Metadata fast-path (if metadata present)
                       +-- Default 8px stride search (fallback)
                       +-- Rotation mode: 16px stride x 24 angles (opt-in)
                       |
                       v
                  LedgerEntry found? --> Verify signatures
                       |
                       +-- cert.signature valid against K_master_pub
                       +-- device_signature valid against cert.device_pub
                       |
                       v
                  Origin, Device, Org, Timestamp PROVEN
```

---

## Watermark Embedding

### Global Radial DFT Modulation (Layer 1 Signal)

The global detection signal is embedded by modifying the radial energy profile of the image's 2D Discrete Fourier Transform. This provides a detection signal that is **rotation-invariant** (radial integration over angles cancels rotation) and **translation-invariant** (DFT magnitude is unaffected by spatial shifts).

1. Convert the image to YCbCr. Extract the Y (luminance) channel.
2. Compute the full-image 2D DFT:
$$F(u,v) = \sum_{x=0}^{W-1} \sum_{y=0}^{H-1} I_Y(x,y) \cdot e^{-i 2\pi \left(\frac{ux}{W} + \frac{vy}{H}\right)}$$
3. Map $|F(u,v)|$ to polar coordinates $(f, \theta)$ via bilinear interpolation.
4. Integrate over all angles to get the 1D radial energy profile:
$$A(f) = \frac{1}{N_\theta} \sum_{k=0}^{N_\theta - 1} |F(f, \theta_k)|$$
5. Compute a coarse 64-bit visual hash of the image (8x8 grayscale, nearest-neighbor resize, threshold against mean). This hash is deterministic and survives most modifications.
6. For each frequency bin $f$ in the target mid-frequency band $[f_{low}, f_{high}]$, compute a per-frequency QIM offset derived from `K_master_pub` and the coarse hash, then apply QIM embedding:
   - Bit 0: $A'(f) = \Delta \cdot \text{round}\left(\frac{A(f) - \text{offset}(f)}{\Delta}\right) + \text{offset}(f)$
   - Bit 1: $A'(f) = \Delta \cdot \text{round}\left(\frac{A(f) - \text{offset}(f) - \Delta/2}{\Delta}\right) + \text{offset}(f) + \frac{\Delta}{2}$
7. Scale the 2D polar magnitudes by $A'(f)/A(f)$ at each radial bin. Phases are preserved.
8. Inverse DFT to reconstruct the spatial-domain image.

When a QIM embedding has two equidistant valid quantization levels for a coefficient, the embedder picks the level that maximizes correlation with the public reference vectors used in Layer 1 detection. This reinforces the detection signal at zero additional distortion cost.

### Per-Tile DCT Coefficient Embedding (Layer 3 Signal)

The image is segmented into a grid of fixed 256x256 pixel tiles:

```
tiles_x = width / 256    (integer division)
tiles_y = height / 256
total_tiles = tiles_x * tiles_y
```

Minimum image size is 256x256 (1 tile). Remainder pixels at the right and bottom edges are unwatermarked.

The same 256-bit payload is embedded independently into every tile. The payload is derived deterministically:

$$\text{payload} = \text{HMAC-SHA256}(K_{device\_secret},\ \text{"wm-payload-v1"} \| \text{pHash\_original}(I))$$

For each tile and each of the 256 payload bits, a per-tile-per-bit seed is derived via HMAC from the spreading seed, and a ChaCha20 PRNG seeded with that value selects 20 coefficient indices from the DCT band [5, 13] (zig-zag indices) across the tile's 1,024 8x8 blocks. Each selected coefficient is QIM-embedded with the payload bit. Decoding uses majority vote across the 20 coefficients.

This gives every tile an independent, complete copy of the full payload. Even if most tiles are destroyed by cropping, a single surviving tile contains the full 256 bits.

### Perceptual Hashing (pHash256)

A 256-bit perceptual hash robust to scale, rotation, and compression:

1. Convert to grayscale. Resize to 32x32 using bilinear interpolation.
2. Apply the 2D DCT to the 32x32 matrix.
3. Extract the top-left 16x16 sub-block, excluding the DC term at (0,0) — yielding 255 coefficients.
4. Compute the mean of these 255 values.
5. Each coefficient above the mean contributes a `1` bit; below contributes `0`. The 256th bit is padded with `0`.

Three distinct pHash computations occur during embedding: `pHash_original` (pre-watermark, used for payload and seed derivation), `pHash_watermarked` (post-watermark full image, stored in ledger), and per-tile pHashes (post-watermark, stored in ledger for MIH search).

---

## Detection and Verification

### Layer 1: Physical Presence Detection

Public, offline, runs in milliseconds. Detects the physical presence of the watermark in the image's frequency domain using only `K_master_pub`. Does not require ledger access.

1. Compute the coarse 64-bit visual hash of the suspect image.
2. Derive $M$ pseudorandom $\pm 1$ reference vectors from `K_master_pub` + coarse hash using HMAC-SHA256 seeds expanded via ChaCha20.
3. Compute the radial DFT profile of the suspect image.
4. For each reference vector $\mathbf{r}_j$, compute the projection correlation:
$$C_j = \sum_{i=0}^{L-1} A'(f_{low} + i) \cdot r_j(i)$$
5. Count positive correlations $N_{pos}$. Under the null hypothesis (no watermark), $N_{pos} \sim B(M, 0.5)$.
6. Compute the upper-tail p-value. Detection threshold: $p < 10^{-6}$.

**Properties**: Rotation-invariant (radial profile integrates over all angles). Translation-invariant (DFT magnitude is unaffected by spatial shifts). Detection SNR scales as $\sqrt{\text{tiles}}$ — a 4K image with 128 tiles has ~11x better SNR than a single tile.

### Layer 2: Ledger Authentication

Matches tile pHashes against the distributed public ledger. Verifies cryptographic signatures to prove who watermarked the image and when.

**The search proceeds in three stages, with early exit on match:**

**Stage 0 — Metadata fast-path.** The verifier checks for a `wm_grid_metadata` chunk in the image metadata (PNG `tEXt` or JPEG `COM`/`APP` marker). If present and valid, it contains the exact grid layout and tile coordinates as JSON. The verifier extracts tiles at those coordinates, computes their pHashes, and queries the ledger directly. If a matching entry is found and its signatures verify, verification is complete — the spatial search is skipped entirely. If the metadata is missing, corrupted, or does not produce a ledger match, the verifier falls through to Stage 1.

**Stage 1 — Default 8px stride search.** The verifier slides a 256x256 extraction window across the image at 8-pixel intervals in both axes, producing 1,024 candidate tile positions. At each position, it computes the tile pHash and queries the ledger via MIH. On first match with valid signatures, verification returns.

**Stage 2 — Rotation fallback (opt-in).** If Stage 1 finds nothing and the caller has opted into rotation mode, the verifier rotates the image at 24 candidate angles (4 cardinal + 20 fine-grained from -5 to +5 degrees in 0.5-degree steps). For each angle, it runs the same search at 16-pixel stride intervals (256 positions per angle), totaling 6,144 lookups.

Once a ledger entry is found, the verifier checks:
1. The certificate signature against `K_master_pub` — proving the device was legitimately provisioned.
2. The entry signature against the certificate's `device_pub` — proving the entry was created by that device.

A successful result proves the image was watermarked by a specific device belonging to a specific organization at a recorded timestamp.

### Layer 3: Device Key Extraction

Requires `K_device_secret`. The device re-derives the spreading seed and payload from the image's pHash, then for each tile, decodes all 256 payload bits via majority vote across the 20 selected DCT coefficients per bit. Reports per-tile match count and confidence.

Used when a device wants to prove it embedded a specific image.

### Layer 4: Authority Extraction

Requires `K_master_secret`. The authority server re-derives the device's private key from the master secret using the `device_id` and `nonce` stored in the certificate, then runs the same extraction as Layer 3.

Used as the last resort for heavily modified images where ledger lookup fails. The authority server can search its internal database of all embeddings to find the correct original pHash.

> [!CAUTION]
> Because Layer 4 extraction requires access to the `K_master_secret`, it is subject to strict operational and legal controls. It is only initiated under **extremely exceptional circumstances**—such as formal court orders or specialized legal requests.

---

## Ledger System

### Entry Structure

Each ledger entry contains:

| Field | Size | Description |
|---|---|---|
| `phash_original` | 32 bytes | pHash of the original image before watermarking |
| `phash_watermarked` | 32 bytes | pHash of the watermarked image |
| `tile_phashes` | 32 bytes x N | Per-tile pHashes (variable count, depends on image size) |
| `tiles_x`, `tiles_y` | 2 bytes each | Grid dimensions |
| `original_width`, `original_height` | 4 bytes each | Original image dimensions in pixels |
| `timestamp_ms` | 8 bytes | Embedding timestamp (ms since Unix epoch) |
| `device_pub_key` | 32 bytes | Ed25519 public key of the embedding device |
| `org_name` | variable | Organization name |
| `device_signature` | 64 bytes | Ed25519 signature over `to_signing_bytes()` |
| `authority_cert` | ~160+ bytes | Full AuthorityCertificate (device_pub, org_name, device_id, expiry_ms, nonce, master signature) |

The `to_signing_bytes()` method produces a deterministic byte sequence covering all fields except `device_signature` (to avoid circular dependency) and `authority_cert` (which has its own independent signature). All integers are little-endian. Variable-length fields are u32 length-prefixed.

### Multi-Index Hashing (MIH)

Near-$O(1)$ lookup for 256-bit pHashes under Hamming distance threshold $d \le 40$:

1. Split each stored 256-bit hash into 8 blocks of 32 bits.
2. Maintain 8 separate hash tables, one per block position.
3. For a query hash $Q$, split it into 8 blocks. By the pigeonhole principle, any stored hash within Hamming distance $d$ must match on at least one block within Hamming distance $\lfloor 40/8 \rfloor = 5$.
4. For each block, generate all 32-bit values within Hamming distance 5 ($\sum_{j=0}^{5} \binom{32}{j} = 242{,}825$ values per block), look up each in the corresponding table, collect candidate entry IDs.
5. Deduplicate candidates across all 8 tables.
6. For each candidate, compute the full 256-bit Hamming distance. Return entries with distance $\le d$.

### Merkle Tree

The ledger is an append-only binary Merkle tree. Each leaf is `SHA256(CBOR-encoded LedgerEntry)`. Internal nodes are `SHA256(left || right)`. The Merkle root is published periodically as a checkpoint. Inclusion proofs consist of $\lceil\log_2(N)\rceil$ sibling hashes from leaf to root.

---

## Security Model

| Threat | Defense |
|---|---|
| **Statistical averaging** — collect many watermarked images, average to find the signal | Per-image pseudorandom coefficient selection via `HMAC(K_device, pHash(I))`. No consistent signal across images. |
| **Pixel manipulation** — modify DCT coefficients to destroy the watermark | Irrelevant after ledger registration. Removing the global radial signal requires destructive noise (PSNR drop 6+ dB). |
| **Metadata stripping** | Proof is in the ledger, not metadata. Metadata is only a fast-path optimization. |
| **Framing** — register a real photograph as AI-generated | Certificate chain: ledger rejects entries without a valid device signature traceable to `K_master_pub`. |
| **Ledger tampering** | Append-only Merkle tree, distributed across independent operators. Modification breaks the root hash. |
| **Ownership dispute / invertibility** | Timestamp anchoring. First registration wins. Spreading seed requires `K_device_secret`. |
| **Oracle attack** — use the verifier to guide removal | Public projection gives yes/no only. Reveals nothing about which coefficients are watermarked. Radial projection aggregates across all frequencies, making gradient-based removal infeasible. |

---

## Robustness

| Modification | Layer 1 Detection | Layer 2 Ledger | Notes |
|---|---|---|---|
| JPEG Q75+ | Survives | Survives | Radial profile stable under mild quantization |
| JPEG Q50 | Survives | Survives | QIM step dominates JPEG quantization noise |
| JPEG Q20 | Marginal | May fail | Spreading factor (majority vote over 20 coefficients) helps |
| Rotation (any angle) | Immune | Needs rotation mode | Radial DFT is rotation-invariant by construction |
| Crop 25% | Survives (weaker) | Survives (8px search) | Signal proportional to surviving area |
| Crop 50% | Survives (weaker) | Survives | One surviving tile is enough for ledger match |
| Resize | Survives with normalization | Survives | Normalize frequency indices by image dimension |
| Color grading | Immune | Immune | Affects DC / very low frequency only |
| Mirroring | Immune | Immune | Radial profile is symmetric under reflection |
| Noise ~ delta/2 | Fails | Fails | But image is visibly degraded (PSNR drop 6-10 dB) |

---

## Crate Structure

```
spanda/
├── Cargo.toml                     # workspace root
├── crates/
│   ├── wm-core/                   # Image processing primitives
│   │   └── src/
│   │       ├── lib.rs             # Re-exports
│   │       ├── color.rs           # RGB <-> YCbCr conversion
│   │       ├── phash.rs           # 256-bit perceptual hash
│   │       ├── dft.rs             # 2D FFT/IFFT (rustfft)
│   │       ├── polar.rs           # Cartesian <-> Polar mapping
│   │       ├── radial.rs          # Radial profile integration & reconstruction
│   │       ├── qim.rs             # QIM embed/decode with offset
│   │       ├── tiles.rs           # Grid calculation, tile extraction
│   │       ├── spreading.rs       # Spreading sequence via ChaCha20
│   │       └── dct.rs             # 8x8 block DCT/IDCT
│   ├── wm-keys/                   # Cryptographic key management
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── master.rs          # MasterPrivateKey, MasterPublicKey
│   │       ├── device.rs          # HKDF device key derivation
│   │       ├── certificates.rs    # AuthorityCertificate generation & verification
│   │       └── signing.rs         # Ed25519 sign/verify, HMAC-SHA256, HKDF
│   ├── wm-detect/                 # Layer 1 detection
│   │   └── src/
│   │       ├── lib.rs             # verify_layer1()
│   │       ├── coarse_hash.rs     # 8x8 grayscale -> 64-bit hash
│   │       └── projection.rs      # Reference vectors, projections, sign test
│   ├── wm-ledger/                 # Ledger client & MIH indexing
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── entry.rs           # LedgerEntry, to_signing_bytes()
│   │       ├── mih.rs             # Multi-Index Hashing
│   │       ├── merkle.rs          # Merkle tree & inclusion proofs
│   │       ├── client.rs          # HTTP ledger client
│   │       └── serialize.rs       # CBOR/JSON serialization
│   ├── wm-embed/                  # CLI embedding binary
│   │   └── src/main.rs
│   └── wm-verify/                 # CLI verification binary
│       └── src/main.rs
├── tests/
│   ├── integration_tests.rs       # End-to-end embed -> verify
│   └── robustness_benchmarks.rs   # JPEG, crop, rotation sweeps
└── K_master_pub.const             # Compile-time public key (32 bytes, hex)
```

### Key Dependencies

| Crate | Dependencies |
|---|---|
| `wm-core` | `rustfft`, `image`, `rand`, `rand_chacha` |
| `wm-keys` | `ed25519-dalek`, `hkdf`, `hmac`, `sha2`, `rand` |
| `wm-detect` | `wm-core`, `wm-keys` |
| `wm-ledger` | `wm-core`, `wm-keys`, `serde`, `serde_json`, `serde_cbor`, `reqwest`, `tokio` |
| `wm-embed` | `wm-core`, `wm-keys`, `wm-ledger`, `clap`, `image`, `tokio` |
| `wm-verify` | `wm-core`, `wm-keys`, `wm-detect`, `wm-ledger`, `clap`, `image`, `tokio` |

---

## Build and Run

```bash
cargo build --release

# Embed a watermark
cargo run --release --bin wm-embed -- \
  --input image.png \
  --output watermarked.png \
  --key device_credentials.bin \
  --ledger-url http://ledger.example.com

# Verify an image
cargo run --release --bin wm-verify -- \
  --input suspect.png \
  --ledger-url http://ledger.example.com
```

---

## Full Specification

The complete technical specification with all mathematical formulas, data structures, function signatures, and algorithmic pseudocode is in [WATERMARK SPEC.md](WATERMARK%20SPEC.md). That document is the authoritative source of truth for the system and contains sufficient detail for an implementor to build the entire codebase from it alone.

---

## License

TBD
