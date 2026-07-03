use wm_keys::{
    generate_master_keypair, derive_device_key, derive_device_pub,
    generate_device_certificate, verify_device_certificate,
    ed25519_sign, ed25519_verify
};

#[test]
fn test_end_to_end_key_provisioning_and_verification() {
    // ────────────────────────────────────────────────────────
    // 1. KEY DERIVATION SERVER PROVISIONING
    // ────────────────────────────────────────────────────────
    
    // Server generates master authority keys
    let (k_master_priv, k_master_pub) = generate_master_keypair();
    
    // Device details sent to the server
    let device_id = b"secure-camera-hardware-id-abc123987";
    let registration_nonce = [42u8; 32];
    
    // Server derives the device keypair
    let k_device_priv = derive_device_key(&k_master_priv, device_id, &registration_nonce);
    let k_device_pub = derive_device_pub(&k_device_priv);
    
    // Server signs and generates the authority certificate
    let expiry_time_ms = 2500000000000u64; // Far future
    let cert = generate_device_certificate(
        &k_master_priv,
        &k_device_pub,
        "Spanda Hardware Division",
        device_id,
        expiry_time_ms,
        &registration_nonce
    );
    
    // ────────────────────────────────────────────────────────
    // 2. DEVICE EMBEDDING & SIGNING
    // ────────────────────────────────────────────────────────
    
    // Image is watermarked, device signs the original pHash to register provenance
    let phash_original = [88u8; 32];
    let signature = ed25519_sign(&k_device_priv.0, &phash_original);
    
    // ────────────────────────────────────────────────────────
    // 3. DECENTRALIZED VERIFICATION (ANY VALIDATOR NODE)
    // ────────────────────────────────────────────────────────
    
    let current_time_ms = 1719888000000u64; // July 2024
    
    // Validator verifies the certificate using ONLY the public master key
    let is_cert_valid = verify_device_certificate(&cert, &k_master_pub, current_time_ms);
    assert!(is_cert_valid, "Certificate signature check failed!");
    
    // Validator verifies the device signature of the pHash using the device's public key
    let is_sig_valid = ed25519_verify(&k_device_pub.0, &phash_original, &signature);
    assert!(is_sig_valid, "Device signature check failed!");
    
    // ────────────────────────────────────────────────────────
    // 4. SECURITY BOUNDS / TAMPER RESILIENCE TESTING
    // ────────────────────────────────────────────────────────
    
    // A. Rejects expired certificates
    let expired_time_ms = 3000000000000u64; // Exceeds expiry_time_ms
    let is_expired_valid = verify_device_certificate(&cert, &k_master_pub, expired_time_ms);
    assert!(!is_expired_valid, "Validator accepted an expired certificate!");
    
    // B. Rejects tampered organization name
    let mut tampered_cert = cert.clone();
    tampered_cert.org_name = "Hackers Corp".to_string();
    let is_tampered_org_valid = verify_device_certificate(&tampered_cert, &k_master_pub, current_time_ms);
    assert!(!is_tampered_org_valid, "Validator accepted a tampered organization name!");
    
    // C. Rejects tampered device public key
    let mut tampered_cert2 = cert.clone();
    tampered_cert2.device_pub[0] ^= 1;
    let is_tampered_pub_valid = verify_device_certificate(&tampered_cert2, &k_master_pub, current_time_ms);
    assert!(!is_tampered_pub_valid, "Validator accepted a tampered device public key!");
    
    // D. Rejects tampered signature
    let mut tampered_signature = signature;
    tampered_signature[0] ^= 1;
    let is_tampered_sig_valid = ed25519_verify(&k_device_pub.0, &phash_original, &tampered_signature);
    assert!(!is_tampered_sig_valid, "Validator accepted a tampered signature!");
}
