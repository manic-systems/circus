//! Nix base32 encoding for sha256 hashes (the `0-9a-z` minus `eotu` alphabet
//! Nix uses for store-path and NAR hashes).

const ALPHABET: &[u8; 32] = b"0123456789abcdfghijklmnpqrsvwxyz";

/// Whether `byte` is a valid Nix base32 character.
#[must_use]
pub const fn is_base32_byte(byte: u8) -> bool {
  matches!(
    byte,
    b'0'
      ..=b'9'
        | b'a'
        | b'b'
        | b'c'
        | b'd'
        | b'f'
        | b'g'
        | b'h'
        | b'i'
        | b'j'
        | b'k'
        | b'l'
        | b'm'
        | b'n'
        | b'p'
        | b'q'
        | b'r'
        | b's'
        | b'v'
        | b'w'
        | b'x'
        | b'y'
        | b'z'
  )
}

/// Encode a 32-byte sha256 digest as its 52-character Nix base32 string.
#[must_use]
pub fn encode_sha256(bytes: &[u8]) -> String {
  let len = 52;
  let mut out = String::with_capacity(len);
  for pos in 0..len {
    let n = len - 1 - pos;
    let bit = n * 5;
    let byte = bit / 8;
    let offset = bit % 8;
    let mut value = u16::from(bytes[byte]) >> offset;
    if byte + 1 < bytes.len() {
      value |= u16::from(bytes[byte + 1]) << (8 - offset);
    }
    out.push(ALPHABET[(value & 0x1F) as usize] as char);
  }
  out
}

/// Decode a 52-character Nix base32 sha256 string into its 32 bytes.
///
/// # Errors
///
/// Returns an error when the input is not 52 valid Nix base32 characters.
pub fn decode_sha256(text: &str) -> Result<Vec<u8>, String> {
  if text.len() != 52 {
    return Err(format!(
      "Nix base32 sha256 hash has {} chars, expected 52",
      text.len()
    ));
  }
  let mut out = vec![0u8; 32];
  for (pos, c) in text.bytes().enumerate() {
    let value = ALPHABET
      .iter()
      .position(|b| *b == c)
      .ok_or_else(|| format!("invalid Nix base32 character {}", c as char))?
      as u16;
    let n = text.len() - 1 - pos;
    let bit = n * 5;
    let byte = bit / 8;
    let offset = bit % 8;
    out[byte] |= (value << offset) as u8;
    if byte + 1 < out.len() && offset > 3 {
      out[byte + 1] |= (value >> (8 - offset)) as u8;
    }
  }
  Ok(out)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn sha256_roundtrip() {
    let bytes: Vec<u8> = (0..32).collect();
    let encoded = encode_sha256(&bytes);
    assert_eq!(encoded.len(), 52);
    assert!(encoded.bytes().all(is_base32_byte));
    assert_eq!(decode_sha256(&encoded).expect("decode"), bytes);
  }

  #[test]
  fn rejects_non_alphabet_and_wrong_length() {
    assert!(!is_base32_byte(b'e'));
    assert!(decode_sha256("tooshort").is_err());
    assert!(decode_sha256(&"e".repeat(52)).is_err());
  }
}
