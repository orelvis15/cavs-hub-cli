//! Git LFS pointer-file parsing.
//!
//! A pointer file is a tiny UTF-8 blob of `key value` lines:
//!
//! ```text
//! version https://git-lfs.github.com/spec/v1
//! oid sha256:4d7a214614ab2935c943f9e0ff69d22eadbb8f32b1258daaa5e2ca24d17e2393
//! size 12345
//! ```
//!
//! The git-lfs spec requires `version` first and keys sorted; we parse a little
//! more leniently (order-insensitive) but require all three fields, a sha256
//! oid and a blob small enough to plausibly be a pointer.

/// Maximum size of a blob we consider a pointer candidate. The LFS spec caps
/// pointer files well below this.
pub const MAX_POINTER_SIZE: u64 = 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pointer {
    /// Lowercase 64-hex sha256 of the content (without the `sha256:` prefix).
    pub oid: String,
    /// Logical size of the content in bytes.
    pub size: u64,
}

/// Parse an LFS pointer from blob bytes. Returns `None` for anything that is
/// not a well-formed pointer (which is the common case: regular small files).
pub fn parse_pointer(blob: &[u8]) -> Option<Pointer> {
    if blob.len() as u64 > MAX_POINTER_SIZE {
        return None;
    }
    let text = std::str::from_utf8(blob).ok()?;

    let mut version_ok = false;
    let mut oid: Option<String> = None;
    let mut size: Option<u64> = None;

    for line in text.lines() {
        let line = line.trim_end_matches('\r');
        if line.is_empty() {
            continue;
        }
        let (key, value) = line.split_once(' ')?;
        match key {
            "version" => {
                // Accept the canonical spec URL and pre-1.0 variants.
                version_ok = value.contains("git-lfs");
            }
            "oid" => {
                let hex = value.strip_prefix("sha256:")?;
                if hex.len() != 64 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
                    return None;
                }
                oid = Some(hex.to_ascii_lowercase());
            }
            "size" => {
                size = value.parse::<u64>().ok();
                size?;
            }
            // Unknown keys (e.g. x-*) are allowed by the spec.
            _ => {}
        }
    }

    if !version_ok {
        return None;
    }
    Some(Pointer {
        oid: oid?,
        size: size?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const OID: &str = "4d7a214614ab2935c943f9e0ff69d22eadbb8f32b1258daaa5e2ca24d17e2393";

    fn pointer_text() -> String {
        format!("version https://git-lfs.github.com/spec/v1\noid sha256:{OID}\nsize 12345\n")
    }

    #[test]
    fn parses_canonical_pointer() {
        let p = parse_pointer(pointer_text().as_bytes()).expect("pointer");
        assert_eq!(p.oid, OID);
        assert_eq!(p.size, 12345);
    }

    #[test]
    fn accepts_crlf_and_extra_keys() {
        let text = format!(
            "version https://git-lfs.github.com/spec/v1\r\noid sha256:{OID}\r\nsize 7\r\nx-meta yes\r\n"
        );
        let p = parse_pointer(text.as_bytes()).expect("pointer");
        assert_eq!(p.size, 7);
    }

    #[test]
    fn rejects_regular_files() {
        assert!(parse_pointer(b"#!/bin/sh\necho hello\n").is_none());
        assert!(parse_pointer(b"").is_none());
        assert!(parse_pointer(&[0u8, 159, 146, 150]).is_none()); // invalid UTF-8
    }

    #[test]
    fn rejects_missing_fields() {
        let no_size = format!("version https://git-lfs.github.com/spec/v1\noid sha256:{OID}\n");
        assert!(parse_pointer(no_size.as_bytes()).is_none());
        let no_version = format!("oid sha256:{OID}\nsize 5\n");
        assert!(parse_pointer(no_version.as_bytes()).is_none());
    }

    #[test]
    fn rejects_bad_oid() {
        let bad = "version https://git-lfs.github.com/spec/v1\noid sha256:zz\nsize 5\n";
        assert!(parse_pointer(bad.as_bytes()).is_none());
        let sha1 = "version https://git-lfs.github.com/spec/v1\noid sha1:4d7a2146\nsize 5\n";
        assert!(parse_pointer(sha1.as_bytes()).is_none());
    }

    #[test]
    fn rejects_oversized_blobs() {
        let mut text = pointer_text();
        text.push_str(&"x".repeat(2048));
        assert!(parse_pointer(text.as_bytes()).is_none());
    }
}
