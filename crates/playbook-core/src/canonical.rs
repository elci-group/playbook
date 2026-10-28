use serde::Serialize;
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

use crate::error::PlaybookError;

pub fn canonical_value(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            let mut out = Map::new();
            for key in keys {
                out.insert(key.clone(), canonical_value(&map[key]));
            }
            Value::Object(out)
        }
        Value::Array(items) => Value::Array(items.iter().map(canonical_value).collect()),
        other => other.clone(),
    }
}

pub fn canonical_bytes(value: &Value) -> Result<Vec<u8>, PlaybookError> {
    serde_json::to_vec(&canonical_value(value)).map_err(PlaybookError::json)
}

pub fn to_canonical_bytes<T: Serialize>(value: &T) -> Result<Vec<u8>, PlaybookError> {
    let raw = serde_json::to_value(value).map_err(PlaybookError::json)?;
    canonical_bytes(&raw)
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    hex_encode(&digest)
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn object_key_order_does_not_change_the_digest() {
        let left = json!({"b": 1, "a": {"d": 2, "c": 3}});
        let right = json!({"a": {"c": 3, "d": 2}, "b": 1});
        assert_eq!(
            sha256_hex(&canonical_bytes(&left).unwrap()),
            sha256_hex(&canonical_bytes(&right).unwrap())
        );
    }
}
