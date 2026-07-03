use ed25519_dalek::{SigningKey, VerifyingKey, Signer, Verifier, Signature};
use hkdf::Hkdf;
use sha2::Sha256;
use hmac::{Hmac, Mac};

type HmacSha256 = Hmac<Sha256>;

/// Signs a message using Ed25519 with a 32-byte private key seed.
/// Returns a 64-byte signature.
pub fn ed25519_sign(key: &[u8; 32], message: &[u8]) -> [u8; 64] {
    let signing_key = SigningKey::from_bytes(key);
    let signature = signing_key.sign(message);
    signature.to_bytes()
}

/// Verifies an Ed25519 signature over a message using a 32-byte public key.
pub fn ed25519_verify(pub_key: &[u8; 32], message: &[u8], signature: &[u8; 64]) -> bool {
    let Ok(verifying_key) = VerifyingKey::from_bytes(pub_key) else {
        return false;
    };
    let sig = Signature::from_bytes(signature);
    verifying_key.verify(message, &sig).is_ok()
}

/// Computes HMAC-SHA256 of the message with the given key.
pub fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; 32] {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC key can be any size");
    mac.update(message);
    let result = mac.finalize();
    result.into_bytes().into()
}

/// Expands a secret using HKDF-SHA256 with the given salt, info, and output length.
pub fn hkdf_sha256(ikm: &[u8], salt: &[u8], info: &[u8], output_len: usize) -> Vec<u8> {
    let hk = Hkdf::<Sha256>::new(Some(salt), ikm);
    let mut okm = vec![0u8; output_len];
    hk.expand(info, &mut okm).expect("HKDF expansion failed");
    okm
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ed25519_sign_verify_round_trip() {
        let private_key = [7u8; 32];
        let message = b"Hello world, verifying watermarks!";

        // Derive public key to verify
        let signing_key = SigningKey::from_bytes(&private_key);
        let public_key = signing_key.verifying_key().to_bytes();

        let sig = ed25519_sign(&private_key, message);
        let verified = ed25519_verify(&public_key, message, &sig);
        
        assert!(verified);

        // Fail case: wrong message
        assert!(!ed25519_verify(&public_key, b"Wrong message", &sig));

        // Fail case: altered signature
        let mut altered_sig = sig;
        altered_sig[0] ^= 1;
        assert!(!ed25519_verify(&public_key, message, &altered_sig));
    }

    #[test]
    fn test_hmac_sha256() {
        let key = b"mysecretkey";
        let msg = b"data";
        let h1 = hmac_sha256(key, msg);
        let h2 = hmac_sha256(key, msg);
        assert_eq!(h1, h2);
    }
}
