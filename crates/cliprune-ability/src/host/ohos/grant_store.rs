//! Bounded parsing of persistent file permissions, independent of the platform bridge.

use std::collections::BTreeMap;

pub const MAX_BYTES: u64 = 1024 * 1024;
pub const MAX_ENTRIES: usize = 4096;

pub fn parse(bytes: &[u8]) -> Result<BTreeMap<String, u32>, String> {
    if bytes.len() as u64 > MAX_BYTES {
        return Err("Saved file grants exceed the 1 MiB limit; select your files again".into());
    }
    let saved: BTreeMap<String, u32> = serde_json::from_slice(bytes).map_err(|error| format!("Cannot parse saved file grants: {error}"))?;
    if saved.len() > MAX_ENTRIES {
        return Err("Too many saved file grants; select your files again".into());
    }
    if saved.iter().any(|(uri, mode)| uri.is_empty() || !matches!(mode, 1 | 3)) {
        return Err("Invalid saved file permission; select your files again".into());
    }
    Ok(saved)
}
