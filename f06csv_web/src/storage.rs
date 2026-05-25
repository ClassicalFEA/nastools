//! Persist the last-uploaded F06 across reloads using `localStorage` plus
//! gzip + base64. We only bother storing files small enough that the
//! base64-encoded compressed payload is comfortably below typical
//! localStorage quotas (~5 MiB).

use std::io::{Read, Write};

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use gloo_storage::{LocalStorage, Storage};
use serde::{Deserialize, Serialize};

/// Storage key for the cached file.
pub const LAST_FILE_KEY: &str = "f06csv_web.last_file.v1";

/// Approximate cap on the compressed (post-base64) payload, in bytes.
/// Most browsers give us roughly 5 MiB of localStorage; 3 MiB leaves
/// breathing room for our other keys.
pub const MAX_STORED_BYTES: usize = 3 * 1024 * 1024;

/// On-disk shape: the original filename plus a gzip'd, base64'd payload.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StoredFile {
  /// Original file name as the user provided it.
  pub name: String,
  /// Gzip-compressed file bytes, base64-encoded for JSON storage.
  pub gz_b64: String,
}

/// Compresses `bytes` to gzip and returns the raw compressed buffer.
fn gzip(bytes: &[u8]) -> Result<Vec<u8>, String> {
  let mut enc = GzEncoder::new(Vec::new(), Compression::default());
  enc
    .write_all(bytes)
    .map_err(|e| format!("gzip write: {e}"))?;
  return enc.finish().map_err(|e| format!("gzip finish: {e}"));
}

/// Decompresses a gzip buffer back to the original bytes.
fn gunzip(gz: &[u8]) -> Result<Vec<u8>, String> {
  let mut out = Vec::new();
  GzDecoder::new(gz)
    .read_to_end(&mut out)
    .map_err(|e| format!("gunzip: {e}"))?;
  return Ok(out);
}

/// Tries to persist `bytes` under `name`. Returns:
/// * `Ok(true)` if stored,
/// * `Ok(false)` if the file was too big after compression and skipped,
/// * `Err(_)` on a real failure (compression, JSON, browser quota).
pub fn save_file(name: &str, bytes: &[u8]) -> Result<bool, String> {
  let gz = gzip(bytes)?;
  let b64 = B64.encode(&gz);
  if b64.len() > MAX_STORED_BYTES {
    LocalStorage::delete(LAST_FILE_KEY);
    return Ok(false);
  }
  let entry = StoredFile {
    name: name.to_owned(),
    gz_b64: b64,
  };
  LocalStorage::set(LAST_FILE_KEY, &entry)
    .map_err(|e| format!("localStorage set: {e}"))?;
  return Ok(true);
}

/// Loads the previously-saved file, if any.
pub fn load_file() -> Option<(String, Vec<u8>)> {
  let entry: StoredFile = LocalStorage::get(LAST_FILE_KEY).ok()?;
  let gz = B64.decode(entry.gz_b64.as_bytes()).ok()?;
  let bytes = gunzip(&gz).ok()?;
  return Some((entry.name, bytes));
}
