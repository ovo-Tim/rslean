use crate::error::{OleanError, OleanResult};

/// Magic bytes at the start of every .olean file.
const OLEAN_MAGIC: &[u8; 5] = b"olean";

/// Current .olean format version.
const OLEAN_VERSION: u8 = 2;

/// Total header size: 5 + 1 + 1 + 33 + 40 + 8 = 88 bytes on 64-bit.
pub const HEADER_SIZE: usize = 88;

/// Parsed .olean file header.
#[derive(Debug)]
pub struct OleanHeader {
    pub version: u8,
    pub flags: u8,
    pub lean_version: String,
    pub githash: String,
    pub base_addr: u64,
}

impl OleanHeader {
    /// Parse the header from the first 88 bytes of an .olean file.
    pub fn parse(data: &[u8]) -> OleanResult<Self> {
        if data.len() < HEADER_SIZE {
            return Err(OleanError::InvalidHeader(format!(
                "file too small: {} bytes (need at least {})",
                data.len(),
                HEADER_SIZE
            )));
        }

        if &data[0..5] != OLEAN_MAGIC {
            return Err(OleanError::InvalidHeader("bad magic bytes".into()));
        }

        let version = data[5];
        if version != OLEAN_VERSION {
            return Err(OleanError::UnsupportedVersion(version));
        }

        let flags = data[6];

        let lean_version = String::from_utf8_lossy(&data[7..40])
            .trim_end_matches('\0')
            .to_string();

        let githash = String::from_utf8_lossy(&data[40..80])
            .trim_end_matches('\0')
            .to_string();

        let base_addr = u64::from_le_bytes(data[80..88].try_into().unwrap());

        Ok(OleanHeader {
            version,
            flags,
            lean_version,
            githash,
            base_addr,
        })
    }

    /// Whether bignums use GMP encoding (vs Lean-native).
    pub fn uses_gmp(&self) -> bool {
        self.flags & 1 != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_magic() {
        let mut data = vec![0u8; HEADER_SIZE];
        data[0..5].copy_from_slice(b"olean");
        data[5] = 2;
        data[6] = 1;
        // lean_version at 7..40
        data[7..11].copy_from_slice(b"4.21");
        // githash at 40..80
        // base_addr at 80..88 (leave as 0)

        let header = OleanHeader::parse(&data).unwrap();
        assert_eq!(header.version, 2);
        assert!(header.uses_gmp());
        assert!(header.lean_version.starts_with("4.21"));
    }

    #[test]
    fn test_bad_magic() {
        let data = vec![0u8; HEADER_SIZE];
        assert!(OleanHeader::parse(&data).is_err());
    }

    #[test]
    fn test_wrong_version() {
        let mut data = vec![0u8; HEADER_SIZE];
        data[0..5].copy_from_slice(b"olean");
        data[5] = 3; // wrong version
        assert!(matches!(
            OleanHeader::parse(&data),
            Err(OleanError::UnsupportedVersion(3))
        ));
    }
}
