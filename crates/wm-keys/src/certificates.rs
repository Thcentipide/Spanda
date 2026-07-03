use serde::{Serialize, Deserialize};
use crate::master::{MasterPrivateKey, MasterPublicKey};
use crate::device::DevicePublicKey;
use crate::signing::{ed25519_sign, ed25519_verify};

mod signature_serde {
    use serde::{self, Serializer, Deserializer, Deserialize};

    pub fn serialize<S>(bytes: &[u8; 64], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_bytes(bytes)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<[u8; 64], D::Error>
    where
        D: Deserializer<'de>,
    {
        let v = Vec::<u8>::deserialize(deserializer)?;
        if v.len() == 64 {
            let mut arr = [0u8; 64];
            arr.copy_from_slice(&v);
            Ok(arr)
        } else {
            Err(serde::de::Error::custom("Expected signature array of length 64"))
        }
    }
}

/// Represents the cryptographic certificate issued by the Master key authority.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct AuthorityCertificate {
    /// 32-byte Ed25519 public key of the device.
    pub device_pub: [u8; 32],
    /// Name of the provisioning organization.
    pub org_name: String,
    /// Hardware/logical unique identifier of the device.
    pub device_id: Vec<u8>,
    /// Expiration timestamp in milliseconds since Unix Epoch.
    pub expiry_ms: u64,
    /// 32-byte registration session nonce.
    pub nonce: [u8; 32],
    /// 64-byte Ed25519 signature computed over the canonical serialization.
    #[serde(with = "signature_serde")]
    pub signature: [u8; 64],
}

impl AuthorityCertificate {
    /// Computes the canonical serialised byte stream of the certificate fields for signing/verification.
    /// Format: `device_pub (32)` || `len(org_name) (u32 LE)` || `org_name` || `len(device_id) (u32 LE)` || `device_id` || `expiry_ms (u64 LE)` || `nonce (32)`
    pub fn to_signing_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(32 + 4 + self.org_name.len() + 4 + self.device_id.len() + 8 + 32);
        bytes.extend_from_slice(&self.device_pub);
        bytes.extend_from_slice(&(self.org_name.len() as u32).to_le_bytes());
        bytes.extend_from_slice(self.org_name.as_bytes());
        bytes.extend_from_slice(&(self.device_id.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&self.device_id);
        bytes.extend_from_slice(&self.expiry_ms.to_le_bytes());
        bytes.extend_from_slice(&self.nonce);
        bytes
    }
}

/// Generates a device certificate signed by the master authority private key.
pub fn generate_device_certificate(
    k_master_secret: &MasterPrivateKey,
    device_pub: &DevicePublicKey,
    org_name: &str,
    device_id: &[u8],
    expiry_ms: u64,
    nonce: &[u8; 32],
) -> AuthorityCertificate {
    let mut cert = AuthorityCertificate {
        device_pub: device_pub.0,
        org_name: org_name.to_string(),
        device_id: device_id.to_vec(),
        expiry_ms,
        nonce: *nonce,
        signature: [0u8; 64],
    };

    let signing_bytes = cert.to_signing_bytes();
    cert.signature = ed25519_sign(&k_master_secret.0, &signing_bytes);
    cert
}

/// Verifies that a device certificate is validly signed by the master key and has not expired.
pub fn verify_device_certificate(
    cert: &AuthorityCertificate,
    k_master_pub: &MasterPublicKey,
    current_time_ms: u64,
) -> bool {
    // Verify expiry bound
    if current_time_ms > cert.expiry_ms {
        return false;
    }

    let signing_bytes = cert.to_signing_bytes();
    ed25519_verify(&k_master_pub.0, &signing_bytes, &cert.signature)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::master::generate_master_keypair;
    use crate::device::derive_device_pub;
    use crate::device::derive_device_key;

    #[test]
    fn test_certificate_sign_and_verify() {
        let (k_master_priv, k_master_pub) = generate_master_keypair();
        let device_id = b"camera-987-xyz";
        let nonce = [77u8; 32];
        let k_device_priv = derive_device_key(&k_master_priv, device_id, &nonce);
        let k_device_pub = derive_device_pub(&k_device_priv);

        let expiry = 2000000000000u64; // Far future
        let cert = generate_device_certificate(
            &k_master_priv,
            &k_device_pub,
            "Spanda Labs",
            device_id,
            expiry,
            &nonce,
        );

        // Verification success
        let ok = verify_device_certificate(&cert, &k_master_pub, 1000000000000u64);
        assert!(ok);

        // Verification fail due to expiry
        let expired = verify_device_certificate(&cert, &k_master_pub, 3000000000000u64);
        assert!(!expired);

        // Verification fail due to tampered field (device_pub)
        let mut tampered = cert.clone();
        tampered.device_pub[0] ^= 1;
        let verified_tampered = verify_device_certificate(&tampered, &k_master_pub, 1000000000000u64);
        assert!(!verified_tampered);

        // Verification fail due to tampered org_name
        let mut tampered_org = cert.clone();
        tampered_org.org_name = "Altered Corp".to_string();
        assert!(!verify_device_certificate(&tampered_org, &k_master_pub, 1000000000000u64));
    }
}
