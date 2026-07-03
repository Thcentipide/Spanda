use serde::{Serialize, Deserialize};
use crate::master::MasterPrivateKey;
use crate::signing::hkdf_sha256;
use ed25519_dalek::SigningKey;

/// Represents a 32-byte device private key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DevicePrivateKey(pub [u8; 32]);

/// Represents a 32-byte device public key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DevicePublicKey(pub [u8; 32]);

/// Derives a device private key from the master secret, device ID, and a 32-byte nonce.
/// Uses HKDF-SHA256 with:
/// - Salt: `nonce`
/// - Info: `"spanda-device-key-v1" || device_id`
pub fn derive_device_key(
    k_master_secret: &MasterPrivateKey,
    device_id: &[u8],
    nonce: &[u8; 32],
) -> DevicePrivateKey {
    let mut info = Vec::with_capacity(20 + device_id.len());
    info.extend_from_slice(b"spanda-device-key-v1");
    info.extend_from_slice(device_id);

    let key_bytes = hkdf_sha256(&k_master_secret.0, nonce, &info, 32);
    let mut key = [0u8; 32];
    key.copy_from_slice(&key_bytes);
    DevicePrivateKey(key)
}

/// Derives the corresponding 32-byte Ed25519 public key from the device private seed.
pub fn derive_device_pub(k_device_secret: &DevicePrivateKey) -> DevicePublicKey {
    let signing_key = SigningKey::from_bytes(&k_device_secret.0);
    DevicePublicKey(signing_key.verifying_key().to_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_device_key_derivation_determinism() {
        let master_priv = MasterPrivateKey([5u8; 32]);
        let device_id = b"camera-sensor-98765";
        let nonce = [12u8; 32];

        let dev_priv1 = derive_device_key(&master_priv, device_id, &nonce);
        let dev_priv2 = derive_device_key(&master_priv, device_id, &nonce);
        assert_eq!(dev_priv1, dev_priv2);

        let dev_pub1 = derive_device_pub(&dev_priv1);
        let dev_pub2 = derive_device_pub(&dev_priv2);
        assert_eq!(dev_pub1, dev_pub2);
    }
}
