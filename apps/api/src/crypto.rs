use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use rand::RngCore;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

// ponytail: simple app-level encryption-at-rest for credential config blobs; key rotation and
// stronger KMS integration are out of scope.

const ENC_MARKER: &str = "__enc_v";

fn encryption_key_material() -> [u8; 32] {
    let key_str = std::env::var("ENCRYPTION_KEY")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "change-me-to-a-secret-key".to_string());
    let digest = Sha256::digest(key_str.as_bytes());
    let mut out = [0u8; 32];
    out.copy_from_slice(&digest);
    out
}

pub fn is_encryption_configured() -> bool {
    // ponytail: true when encryption support is active; explicit external key management not enforced.
    std::env::var("ENCRYPTION_KEY")
        .ok()
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false)
}

pub fn is_encrypted_config(v: &Value) -> bool {
    v.get(ENC_MARKER).and_then(|x| x.as_i64()) == Some(1)
        && v.get("nonce").is_some()
        && v.get("ciphertext").is_some()
}

pub fn encrypt_config(plain: &Value) -> Result<Value, String> {
    if is_encrypted_config(plain) {
        return Ok(plain.clone());
    }
    let plaintext = serde_json::to_vec(plain).map_err(|e| e.to_string())?;
    let cipher = Aes256Gcm::new_from_slice(&encryption_key_material()).map_err(|e| e.to_string())?;
    let mut nonce_bytes = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher.encrypt(nonce, plaintext.as_ref()).map_err(|e| format!("encrypt failed: {e}"))?;
    Ok(json!({
        ENC_MARKER: 1,
        "nonce": B64.encode(nonce_bytes),
        "ciphertext": B64.encode(ciphertext),
    }))
}

pub fn decrypt_config(stored: &Value) -> Result<Value, String> {
    if !is_encrypted_config(stored) {
        return Ok(stored.clone());
    }
    let nonce_b64 = stored.get("nonce").and_then(|v| v.as_str()).ok_or("missing nonce")?;
    let ct_b64 = stored.get("ciphertext").and_then(|v| v.as_str()).ok_or("missing ciphertext")?;
    let nonce_bytes = B64.decode(nonce_b64).map_err(|e| format!("invalid nonce: {e}"))?;
    let ciphertext = B64.decode(ct_b64).map_err(|e| format!("invalid ciphertext: {e}"))?;
    if nonce_bytes.len() != 12 {
        return Err("invalid nonce length".into());
    }
    let cipher = Aes256Gcm::new_from_slice(&encryption_key_material()).map_err(|e| e.to_string())?;
    let nonce = Nonce::from_slice(&nonce_bytes);
    let plaintext = cipher.decrypt(nonce, ciphertext.as_ref()).map_err(|_| "decryption failed (key mismatch or corrupted data)".to_string())?;
    serde_json::from_slice(&plaintext).map_err(|e| format!("invalid decrypted config: {e}"))
}
