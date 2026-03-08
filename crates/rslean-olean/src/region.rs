use crate::error::{OleanError, OleanResult};

/// Lean object tags for special (non-constructor) objects.
#[allow(dead_code)]
pub const LEAN_MAX_CTOR_TAG: u8 = 243;
#[allow(dead_code)]
pub const LEAN_TAG_CLOSURE: u8 = 245;
pub const LEAN_TAG_ARRAY: u8 = 246;
#[allow(dead_code)]
pub const LEAN_TAG_STRUCT_ARRAY: u8 = 247;
#[allow(dead_code)]
pub const LEAN_TAG_SCALAR_ARRAY: u8 = 248;
pub const LEAN_TAG_STRING: u8 = 249;
pub const LEAN_TAG_MPZ: u8 = 250;

/// A reference to a Lean object — either a scalar value or a position in the region.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ObjRef {
    /// Null pointer (0).
    Null,
    /// A scalar value (the stored raw value was `(val << 1) | 1`).
    /// For enum-like constructors (Bool, BinderInfo, etc.), this is the tag.
    /// For small Nats, this is the numeric value.
    Scalar(u64),
    /// An object at a byte position within the region data.
    Ptr(usize),
}

/// Low-level reader for a compacted object region.
///
/// Objects are stored contiguously. Pointers within objects are encoded as
/// offsets from a base address. Scalar values have bit 0 set.
pub struct CompactedRegion {
    /// Raw bytes of the region (everything after the 88-byte header).
    data: Vec<u8>,
    /// Base address for offset computation: `header.base_addr + HEADER_SIZE`.
    base_addr: u64,
    /// Whether bignums use GMP encoding.
    pub uses_gmp: bool,
}

impl CompactedRegion {
    pub fn new(data: Vec<u8>, file_base_addr: u64, uses_gmp: bool) -> Self {
        CompactedRegion {
            data,
            base_addr: file_base_addr + crate::header::HEADER_SIZE as u64,
            uses_gmp,
        }
    }

    /// Get the root object reference (first 8 bytes of region data).
    pub fn root(&self) -> ObjRef {
        if self.data.len() < 8 {
            return ObjRef::Null;
        }
        let raw = self.read_u64(0);
        self.resolve(raw)
    }

    /// Convert a raw stored pointer/offset value to an ObjRef.
    pub fn resolve(&self, raw: u64) -> ObjRef {
        if raw == 0 {
            ObjRef::Null
        } else if raw & 1 != 0 {
            ObjRef::Scalar(raw >> 1)
        } else {
            // Convert offset to position within region data.
            let pos = raw.wrapping_sub(self.base_addr) as usize;
            if pos >= self.data.len() {
                ObjRef::Null
            } else {
                ObjRef::Ptr(pos)
            }
        }
    }

    #[inline]
    fn check_bounds(&self, pos: usize, len: usize) -> OleanResult<()> {
        if pos + len > self.data.len() {
            Err(OleanError::OutOfBounds {
                pos,
                size: self.data.len(),
            })
        } else {
            Ok(())
        }
    }

    #[inline]
    pub fn read_u8(&self, pos: usize) -> u8 {
        self.data[pos]
    }

    #[inline]
    pub fn read_u16(&self, pos: usize) -> u16 {
        u16::from_le_bytes(self.data[pos..pos + 2].try_into().unwrap())
    }

    #[inline]
    pub fn read_u32(&self, pos: usize) -> u32 {
        u32::from_le_bytes(self.data[pos..pos + 4].try_into().unwrap())
    }

    #[inline]
    pub fn read_u64(&self, pos: usize) -> u64 {
        u64::from_le_bytes(self.data[pos..pos + 8].try_into().unwrap())
    }

    // ─── Object header access ───────────────────────────────────────────
    // Layout: [m_rc: i32 (4)] [m_cs_sz: u16 (2)] [m_other: u8 (1)] [m_tag: u8 (1)]

    /// Constructor tag (byte 7 of the object header).
    #[inline]
    pub fn obj_tag(&self, pos: usize) -> u8 {
        self.data[pos + 7]
    }

    /// Number of object (pointer) fields (byte 6 of the object header).
    #[inline]
    pub fn obj_num_objs(&self, pos: usize) -> u8 {
        self.data[pos + 6]
    }

    /// Compact size (`m_cs_sz`) — total byte size for small objects.
    #[inline]
    pub fn obj_cs_sz(&self, pos: usize) -> u16 {
        self.read_u16(pos + 4)
    }

    // ─── Constructor fields ─────────────────────────────────────────────

    /// Read the i-th object (pointer) field of a constructor at `pos`.
    #[inline]
    pub fn ctor_obj_field(&self, pos: usize, i: usize) -> ObjRef {
        let field_pos = pos + 8 + i * 8;
        let raw = self.read_u64(field_pos);
        self.resolve(raw)
    }

    /// Read a u8 scalar field. `scalar_offset` is relative to the start of
    /// the scalar area (after all object fields).
    #[inline]
    pub fn ctor_scalar_u8(&self, pos: usize, num_objs: usize, scalar_offset: usize) -> u8 {
        self.read_u8(pos + 8 + num_objs * 8 + scalar_offset)
    }

    /// Read a u32 scalar field.
    #[inline]
    pub fn ctor_scalar_u32(&self, pos: usize, num_objs: usize, scalar_offset: usize) -> u32 {
        self.read_u32(pos + 8 + num_objs * 8 + scalar_offset)
    }

    /// Read a u64 scalar field.
    #[inline]
    pub fn ctor_scalar_u64(&self, pos: usize, num_objs: usize, scalar_offset: usize) -> u64 {
        self.read_u64(pos + 8 + num_objs * 8 + scalar_offset)
    }

    // ─── Array (tag 246) ────────────────────────────────────────────────
    // Layout: [header: 8] [m_size: u64] [m_capacity: u64] [elements: m_size × u64]

    pub fn array_size(&self, pos: usize) -> usize {
        self.read_u64(pos + 8) as usize
    }

    pub fn array_elem(&self, pos: usize, i: usize) -> ObjRef {
        let elem_pos = pos + 24 + i * 8; // header(8) + size(8) + capacity(8) + i*8
        let raw = self.read_u64(elem_pos);
        self.resolve(raw)
    }

    // ─── String (tag 249) ───────────────────────────────────────────────
    // Layout: [header: 8] [m_size: u64] [m_capacity: u64] [m_length: u64] [data: m_size bytes]

    pub fn string_byte_size(&self, pos: usize) -> usize {
        self.read_u64(pos + 8) as usize
    }

    /// Read a string value (UTF-8, not including the null terminator).
    pub fn string_value(&self, pos: usize) -> OleanResult<&str> {
        let byte_size = self.string_byte_size(pos);
        if byte_size == 0 {
            return Ok("");
        }
        let data_pos = pos + 32; // header(8) + size(8) + capacity(8) + length(8)
        self.check_bounds(data_pos, byte_size)?;
        let bytes = &self.data[data_pos..data_pos + byte_size - 1]; // -1 for null terminator
        std::str::from_utf8(bytes)
            .map_err(|e| OleanError::Deserialize(format!("invalid UTF-8 string: {}", e)))
    }

    // ─── MPZ (tag 250) ──────────────────────────────────────────────────

    /// Read a GMP-format MPZ as u64. Returns Err for values that don't fit.
    pub fn mpz_to_u64_gmp(&self, pos: usize) -> OleanResult<u64> {
        // GMP __mpz_struct layout after the 8-byte object header:
        //   _mp_alloc: i32 (4 bytes)
        //   _mp_size: i32 (4 bytes) — number of limbs (negative = negative number)
        //   _mp_d: pointer (8 bytes) — pointer to limb array (already an offset)
        let mp_size = self.read_u32(pos + 12) as i32; // bytes 12-15
        let limb_ptr_raw = self.read_u64(pos + 16); // bytes 16-23

        if mp_size < 0 {
            return Err(OleanError::Deserialize(
                "negative MPZ not supported as u64".into(),
            ));
        }
        if mp_size == 0 {
            return Ok(0);
        }

        let limb_pos = self.resolve(limb_ptr_raw);
        match limb_pos {
            ObjRef::Ptr(lpos) => {
                if mp_size == 1 {
                    Ok(self.read_u64(lpos))
                } else {
                    Err(OleanError::Deserialize(format!(
                        "MPZ with {} limbs too large for u64",
                        mp_size
                    )))
                }
            }
            _ => Err(OleanError::Deserialize("invalid MPZ limb pointer".into())),
        }
    }

    /// Read a non-GMP (Lean-native) MPZ as u64.
    pub fn mpz_to_u64_native(&self, pos: usize) -> OleanResult<u64> {
        // Native mpn layout after the 8-byte object header:
        //   m_digits: pointer (8 bytes) — points right after the struct
        //   m_size: u64 (8 bytes)
        //   m_sign: u8 (1 byte)
        let m_size = self.read_u64(pos + 16) as usize;
        let m_sign = self.read_u8(pos + 24);

        if m_sign != 0 {
            return Err(OleanError::Deserialize(
                "negative MPZ not supported as u64".into(),
            ));
        }
        if m_size == 0 {
            return Ok(0);
        }

        // Digits follow immediately after the struct.
        // The struct is: header(8) + digits_ptr(8) + size(8) + sign(1) + padding
        // sizeof(mpz_object) is 32 bytes (8+8+8+8) for native
        let digit_pos = pos + 32;
        if m_size == 1 {
            Ok(self.read_u64(digit_pos))
        } else {
            Err(OleanError::Deserialize(format!(
                "MPZ with {} digits too large for u64",
                m_size
            )))
        }
    }

    /// Read an MPZ value as u64, choosing GMP or native format.
    pub fn mpz_to_u64(&self, pos: usize) -> OleanResult<u64> {
        if self.uses_gmp {
            self.mpz_to_u64_gmp(pos)
        } else {
            self.mpz_to_u64_native(pos)
        }
    }

    pub fn data_len(&self) -> usize {
        self.data.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_scalar() {
        let region = CompactedRegion::new(vec![0; 64], 0, false);
        // Scalar value: (42 << 1) | 1 = 85
        assert_eq!(region.resolve(85), ObjRef::Scalar(42));
    }

    #[test]
    fn test_resolve_null() {
        let region = CompactedRegion::new(vec![0; 64], 0, false);
        assert_eq!(region.resolve(0), ObjRef::Null);
    }

    #[test]
    fn test_resolve_ptr() {
        // base_addr = 1000 + 88 = 1088
        // stored offset = 1088 + 16 = 1104
        let region = CompactedRegion::new(vec![0; 64], 1000, false);
        assert_eq!(region.resolve(1104), ObjRef::Ptr(16));
    }
}
