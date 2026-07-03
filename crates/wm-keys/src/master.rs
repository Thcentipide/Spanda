use serde::{Serialize, Deserialize};
use ed25519_dalek::SigningKey;

/// Represents a 32-byte master private key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MasterPrivateKey(pub [u8; 32]);

/// Represents a 32-byte master public key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MasterPublicKey(pub [u8; 32]);

/// Generates a new random Master Ed25519 keypair.
pub fn generate_master_keypair() -> (MasterPrivateKey, MasterPublicKey) {
    let mut rng = rand::thread_rng();
    let signing_key = SigningKey::generate(&mut rng);
    
    (
        MasterPrivateKey(signing_key.to_bytes()),
        MasterPublicKey(signing_key.verifying_key().to_bytes()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_master_key_generation() {
        let (priv_key, pub_key) = generate_master_keypair();
        assert_ne!(priv_key.0, [0u8; 32]);
        assert_ne!(pub_key.0, [0u8; 32]);
    }
}
