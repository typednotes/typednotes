//! Cryptographic utilities for encrypting SSH keys at rest.

use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use rand::RngCore;

/// Read and validate the 32-byte master encryption key from `ENCRYPTION_KEY` env var.
fn get_master_key() -> Result<[u8; 32], String> {
    let hex_key =
        std::env::var("ENCRYPTION_KEY").map_err(|_| "ENCRYPTION_KEY env var not set".to_string())?;
    let bytes = hex::decode(&hex_key).map_err(|e| format!("Invalid ENCRYPTION_KEY hex: {}", e))?;
    if bytes.len() != 32 {
        return Err(format!(
            "ENCRYPTION_KEY must be 64 hex chars (32 bytes), got {} bytes",
            bytes.len()
        ));
    }
    let mut key = [0u8; 32];
    key.copy_from_slice(&bytes);
    Ok(key)
}

/// Encrypt data using AES-256-GCM with a random 12-byte nonce.
/// Returns (ciphertext, nonce).
pub fn encrypt_ssh_key(plaintext: &[u8]) -> Result<(Vec<u8>, Vec<u8>), String> {
    let key = get_master_key()?;
    let cipher = Aes256Gcm::new_from_slice(&key)
        .map_err(|e| format!("Failed to create cipher: {}", e))?;

    let mut nonce_bytes = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, plaintext)
        .map_err(|e| format!("Encryption failed: {}", e))?;

    Ok((ciphertext, nonce_bytes.to_vec()))
}

/// Decrypt data using AES-256-GCM.
pub fn decrypt_ssh_key(ciphertext: &[u8], nonce: &[u8]) -> Result<Vec<u8>, String> {
    let key = get_master_key()?;
    let cipher = Aes256Gcm::new_from_slice(&key)
        .map_err(|e| format!("Failed to create cipher: {}", e))?;

    let nonce = Nonce::from_slice(nonce);
    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| format!("Decryption failed: {}", e))?;

    Ok(plaintext)
}

/// Extract the public key from an SSH private key in PEM/OpenSSH format.
/// Returns the public key in OpenSSH format.
pub fn extract_public_key(private_key_pem: &str) -> Result<String, String> {
    let private_key = ssh_key::PrivateKey::from_openssh(private_key_pem.trim())
        .map_err(|e| format!("Invalid SSH private key: {}", e))?;
    let public_key = private_key.public_key();
    Ok(public_key.to_openssh().map_err(|e| format!("Failed to format public key: {}", e))?)
}
