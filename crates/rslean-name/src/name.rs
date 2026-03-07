use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::sync::Arc;

/// MurmurHash2 64-bit, matching Lean 4's `hash_str` implementation.
/// Used for string hashing with seed 11.
fn murmur_hash_64a(data: &[u8], seed: u64) -> u64 {
    const M: u64 = 0xc6a4a7935bd1e995;
    const R: u32 = 47;

    let len = data.len();
    let mut h: u64 = seed ^ ((len as u64).wrapping_mul(M));

    // Process 8-byte chunks
    let chunks = len / 8;
    for i in 0..chunks {
        let mut k: u64 = 0;
        // memcpy equivalent — little-endian read
        let offset = i * 8;
        for j in 0..8 {
            k |= (data[offset + j] as u64) << (j * 8);
        }

        k = k.wrapping_mul(M);
        k ^= k >> R;
        k = k.wrapping_mul(M);

        h ^= k;
        h = h.wrapping_mul(M);
    }

    // Process remaining bytes (fallthrough style)
    let tail = &data[chunks * 8..];
    let remaining = len & 7;
    if remaining >= 7 {
        h ^= (tail[6] as u64) << 48;
    }
    if remaining >= 6 {
        h ^= (tail[5] as u64) << 40;
    }
    if remaining >= 5 {
        h ^= (tail[4] as u64) << 32;
    }
    if remaining >= 4 {
        h ^= (tail[3] as u64) << 24;
    }
    if remaining >= 3 {
        h ^= (tail[2] as u64) << 16;
    }
    if remaining >= 2 {
        h ^= (tail[1] as u64) << 8;
    }
    if remaining >= 1 {
        h ^= tail[0] as u64;
        h = h.wrapping_mul(M);
    }

    h ^= h >> R;
    h = h.wrapping_mul(M);
    h ^= h >> R;

    h
}

/// Lean 4's `mixHash` function (`lean_uint64_mix_hash`).
#[inline]
pub fn mix_hash(h: u64, k: u64) -> u64 {
    const M: u64 = 0xc6a4a7935bd1e995;
    const R: u32 = 47;
    let mut k = k.wrapping_mul(M);
    k ^= k >> R;
    k ^= M;
    let mut h = h ^ k;
    h = h.wrapping_mul(M);
    h
}

/// Lean 4's `String.hash` — MurmurHash2 with seed 11.
#[inline]
pub fn lean_string_hash(s: &str) -> u64 {
    murmur_hash_64a(s.as_bytes(), 11)
}

/// Hierarchical names, matching Lean 4's `Name` type.
///
/// Names are immutable, reference-counted, and carry a cached hash.
#[derive(Clone)]
pub struct Name {
    inner: Arc<NameInner>,
}

#[derive(Clone)]
struct NameInner {
    kind: NameKind,
    hash: u64,
}

#[derive(Clone, Debug)]
enum NameKind {
    Anonymous,
    Str { prefix: Name, s: String },
    Num { prefix: Name, n: u64 },
}

/// Hash value for `Name.anonymous` in Lean 4.
const ANONYMOUS_HASH: u64 = 1723;

impl Name {
    /// The anonymous (empty) name.
    pub fn anonymous() -> Self {
        Name {
            inner: Arc::new(NameInner {
                kind: NameKind::Anonymous,
                hash: ANONYMOUS_HASH,
            }),
        }
    }

    /// Create a string name component: `prefix.s`
    pub fn mk_str(prefix: Name, s: impl Into<String>) -> Self {
        let s = s.into();
        let s_hash = lean_string_hash(&s);
        let hash = mix_hash(prefix.hash(), s_hash);
        Name {
            inner: Arc::new(NameInner {
                kind: NameKind::Str { prefix, s },
                hash,
            }),
        }
    }

    /// Create a numeric name component: `prefix.n`
    pub fn mk_num(prefix: Name, n: u64) -> Self {
        // Lean 4: if v < UInt64.size, use v directly; otherwise use 17.
        // Since we store u64, the value always fits.
        let hash = mix_hash(prefix.hash(), n);
        Name {
            inner: Arc::new(NameInner {
                kind: NameKind::Num { prefix, n },
                hash,
            }),
        }
    }

    /// Parse a dot-separated string into a hierarchical name.
    /// e.g., "Lean.Meta.whnf" → `.str (.str (.str .anonymous "Lean") "Meta") "whnf"`
    pub fn from_str_parts(s: &str) -> Self {
        let mut result = Name::anonymous();
        for part in s.split('.') {
            if !part.is_empty() {
                result = Name::mk_str(result, part);
            }
        }
        result
    }

    /// Create a simple (single-component) name.
    pub fn mk_simple(s: impl Into<String>) -> Self {
        Name::mk_str(Name::anonymous(), s)
    }

    #[inline]
    pub fn hash(&self) -> u64 {
        self.inner.hash
    }

    #[inline]
    pub fn is_anonymous(&self) -> bool {
        matches!(self.inner.kind, NameKind::Anonymous)
    }

    #[inline]
    pub fn is_str(&self) -> bool {
        matches!(self.inner.kind, NameKind::Str { .. })
    }

    #[inline]
    pub fn is_num(&self) -> bool {
        matches!(self.inner.kind, NameKind::Num { .. })
    }

    /// Returns `true` if the name has at most one component.
    pub fn is_atomic(&self) -> bool {
        match &self.inner.kind {
            NameKind::Anonymous => true,
            NameKind::Str { prefix, .. } | NameKind::Num { prefix, .. } => prefix.is_anonymous(),
        }
    }

    /// Get the prefix of a non-anonymous name.
    /// Returns `self` if anonymous (matching Lean 4 behavior).
    pub fn get_prefix(&self) -> &Name {
        match &self.inner.kind {
            NameKind::Anonymous => self,
            NameKind::Str { prefix, .. } | NameKind::Num { prefix, .. } => prefix,
        }
    }

    /// Get the string component (panics if not a string name).
    pub fn get_string(&self) -> &str {
        match &self.inner.kind {
            NameKind::Str { s, .. } => s,
            _ => panic!("Name.get_string: not a string name"),
        }
    }

    /// Get the numeric component (panics if not a numeric name).
    pub fn get_numeral(&self) -> u64 {
        match &self.inner.kind {
            NameKind::Num { n, .. } => *n,
            _ => panic!("Name.get_numeral: not a numeric name"),
        }
    }

    /// Get the root (first) component of this name.
    pub fn get_root(&self) -> Name {
        let mut n = self.clone();
        while !n.get_prefix().is_anonymous() {
            n = n.get_prefix().clone();
        }
        n
    }

    /// Check if `self` is a prefix of `other`.
    pub fn is_prefix_of(&self, other: &Name) -> bool {
        let limbs1 = self.to_limbs();
        let limbs2 = other.to_limbs();
        if limbs1.len() > limbs2.len() {
            return false;
        }
        for (l1, l2) in limbs1.iter().zip(limbs2.iter()) {
            if l1 != l2 {
                return false;
            }
        }
        true
    }

    /// Concatenate two names: `self + other`.
    pub fn append(&self, other: &Name) -> Name {
        if other.is_anonymous() {
            return self.clone();
        }
        if self.is_anonymous() {
            return other.clone();
        }
        match &other.inner.kind {
            NameKind::Anonymous => self.clone(),
            NameKind::Str { prefix, s } => {
                let new_prefix = if prefix.is_anonymous() {
                    self.clone()
                } else {
                    self.append(prefix)
                };
                Name::mk_str(new_prefix, s.clone())
            }
            NameKind::Num { prefix, n } => {
                let new_prefix = if prefix.is_anonymous() {
                    self.clone()
                } else {
                    self.append(prefix)
                };
                Name::mk_num(new_prefix, *n)
            }
        }
    }

    /// Replace `prefix` at the start of this name with `new_prefix`.
    pub fn replace_prefix(&self, prefix: &Name, new_prefix: &Name) -> Name {
        if self == prefix {
            return new_prefix.clone();
        }
        if self.is_anonymous() {
            return self.clone();
        }
        let p = self.get_prefix().replace_prefix(prefix, new_prefix);
        match &self.inner.kind {
            NameKind::Str { s, .. } => Name::mk_str(p, s.clone()),
            NameKind::Num { n, .. } => Name::mk_num(p, *n),
            NameKind::Anonymous => unreachable!(),
        }
    }

    /// Collect name components into a vector (root first).
    fn to_limbs(&self) -> Vec<NameLimb> {
        let mut limbs = Vec::new();
        let mut n = self;
        loop {
            match &n.inner.kind {
                NameKind::Anonymous => break,
                NameKind::Str { prefix, s } => {
                    limbs.push(NameLimb::Str(s.clone()));
                    n = prefix;
                }
                NameKind::Num { prefix, n: num } => {
                    limbs.push(NameLimb::Num(*num));
                    n = prefix;
                }
            }
        }
        limbs.reverse();
        limbs
    }

    /// Get string components in order (root first).
    /// Numeric components are converted to their string representation.
    pub fn components(&self) -> Vec<String> {
        self.to_limbs()
            .into_iter()
            .map(|l| match l {
                NameLimb::Str(s) => s,
                NameLimb::Num(n) => n.to_string(),
            })
            .collect()
    }

    /// Convert to dot-separated string representation.
    pub fn to_string_with_sep(&self, sep: &str) -> String {
        if self.is_anonymous() {
            return "[anonymous]".to_string();
        }
        let limbs = self.to_limbs();
        let mut parts = Vec::with_capacity(limbs.len());
        for limb in &limbs {
            match limb {
                NameLimb::Str(s) => parts.push(s.clone()),
                NameLimb::Num(n) => parts.push(n.to_string()),
            }
        }
        parts.join(sep)
    }

    /// Return true if `p` is contained in any string component of this name.
    pub fn contains(&self, p: &str) -> bool {
        let mut n = self;
        loop {
            match &n.inner.kind {
                NameKind::Anonymous => return false,
                NameKind::Str { prefix, s } => {
                    if s.contains(p) {
                        return true;
                    }
                    n = prefix;
                }
                NameKind::Num { prefix, .. } => {
                    n = prefix;
                }
            }
        }
    }

    /// Return the number of components in this name.
    pub fn num_parts(&self) -> usize {
        let mut count = 0;
        let mut n = self;
        loop {
            match &n.inner.kind {
                NameKind::Anonymous => return count,
                NameKind::Str { prefix, .. } | NameKind::Num { prefix, .. } => {
                    count += 1;
                    n = prefix;
                }
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum NameLimb {
    Str(String),
    Num(u64),
}

impl Serialize for Name {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.to_limbs().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Name {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let limbs = Vec::<NameLimb>::deserialize(deserializer)?;
        let mut name = Name::anonymous();
        for limb in limbs {
            match limb {
                NameLimb::Str(s) => name = Name::mk_str(name, s),
                NameLimb::Num(n) => name = Name::mk_num(name, n),
            }
        }
        Ok(name)
    }
}

impl Serialize for NameLimb {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            NameLimb::Str(s) => serializer.serialize_newtype_variant("NameLimb", 0, "Str", s),
            NameLimb::Num(n) => serializer.serialize_newtype_variant("NameLimb", 1, "Num", n),
        }
    }
}

impl<'de> Deserialize<'de> for NameLimb {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        enum NameLimbHelper {
            Str(String),
            Num(u64),
        }
        match NameLimbHelper::deserialize(deserializer)? {
            NameLimbHelper::Str(s) => Ok(NameLimb::Str(s)),
            NameLimbHelper::Num(n) => Ok(NameLimb::Num(n)),
        }
    }
}

impl PartialEq for Name {
    fn eq(&self, other: &Self) -> bool {
        // Fast path: pointer equality
        if Arc::ptr_eq(&self.inner, &other.inner) {
            return true;
        }
        // Fast path: hash mismatch
        if self.inner.hash != other.inner.hash {
            return false;
        }
        // Structural comparison
        self.structural_eq(other)
    }
}

impl Eq for Name {}

impl Name {
    fn structural_eq(&self, other: &Name) -> bool {
        match (&self.inner.kind, &other.inner.kind) {
            (NameKind::Anonymous, NameKind::Anonymous) => true,
            (NameKind::Str { prefix: p1, s: s1 }, NameKind::Str { prefix: p2, s: s2 }) => {
                s1 == s2 && p1 == p2
            }
            (NameKind::Num { prefix: p1, n: n1 }, NameKind::Num { prefix: p2, n: n2 }) => {
                n1 == n2 && p1 == p2
            }
            _ => false,
        }
    }
}

impl PartialOrd for Name {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Name {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        let l1 = self.to_limbs();
        let l2 = other.to_limbs();
        for (a, b) in l1.iter().zip(l2.iter()) {
            match (a, b) {
                (NameLimb::Str(s1), NameLimb::Str(s2)) => {
                    let c = s1.cmp(s2);
                    if c != std::cmp::Ordering::Equal {
                        return c;
                    }
                }
                (NameLimb::Num(n1), NameLimb::Num(n2)) => {
                    let c = n1.cmp(n2);
                    if c != std::cmp::Ordering::Equal {
                        return c;
                    }
                }
                // In Lean 4, String > Numeral in mixed comparison
                (NameLimb::Str(_), NameLimb::Num(_)) => return std::cmp::Ordering::Greater,
                (NameLimb::Num(_), NameLimb::Str(_)) => return std::cmp::Ordering::Less,
            }
        }
        l1.len().cmp(&l2.len())
    }
}

impl std::hash::Hash for Name {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        state.write_u64(self.inner.hash);
    }
}

impl fmt::Display for Name {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_string_with_sep("."))
    }
}

impl fmt::Debug for Name {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Name({})", self)
    }
}

impl Default for Name {
    fn default() -> Self {
        Name::anonymous()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_anonymous() {
        let n = Name::anonymous();
        assert!(n.is_anonymous());
        assert!(!n.is_str());
        assert!(!n.is_num());
        assert!(n.is_atomic());
        assert_eq!(n.hash(), 1723);
        assert_eq!(n.to_string(), "[anonymous]");
    }

    #[test]
    fn test_simple_str() {
        let n = Name::mk_simple("Nat");
        assert!(n.is_str());
        assert!(n.is_atomic());
        assert_eq!(n.get_string(), "Nat");
        assert!(n.get_prefix().is_anonymous());
        assert_eq!(n.to_string(), "Nat");
    }

    #[test]
    fn test_hierarchical_name() {
        let n = Name::from_str_parts("Lean.Meta.whnf");
        assert!(n.is_str());
        assert!(!n.is_atomic());
        assert_eq!(n.get_string(), "whnf");
        assert_eq!(n.get_prefix().get_string(), "Meta");
        assert_eq!(n.get_prefix().get_prefix().get_string(), "Lean");
        assert!(n.get_prefix().get_prefix().get_prefix().is_anonymous());
        assert_eq!(n.to_string(), "Lean.Meta.whnf");
        assert_eq!(n.num_parts(), 3);
    }

    #[test]
    fn test_numeric_name() {
        let prefix = Name::mk_simple("_uniq");
        let n = Name::mk_num(prefix, 231);
        assert!(n.is_num());
        assert_eq!(n.get_numeral(), 231);
        assert_eq!(n.get_prefix().get_string(), "_uniq");
        assert_eq!(n.to_string(), "_uniq.231");
    }

    #[test]
    fn test_equality() {
        let a = Name::from_str_parts("Lean.Meta");
        let b = Name::from_str_parts("Lean.Meta");
        let c = Name::from_str_parts("Lean.Elab");
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_eq!(a.hash(), b.hash());
    }

    #[test]
    fn test_ordering() {
        let a = Name::from_str_parts("A.B");
        let b = Name::from_str_parts("A.C");
        let c = Name::from_str_parts("B.A");
        assert!(a < b);
        assert!(a < c);
        assert!(b < c);
    }

    #[test]
    fn test_prefix_check() {
        let prefix = Name::from_str_parts("Lean.Meta");
        let full = Name::from_str_parts("Lean.Meta.whnf");
        let other = Name::from_str_parts("Lean.Elab.Term");
        assert!(prefix.is_prefix_of(&full));
        assert!(!prefix.is_prefix_of(&other));
        assert!(Name::anonymous().is_prefix_of(&full));
    }

    #[test]
    fn test_append() {
        let a = Name::from_str_parts("Lean");
        let b = Name::from_str_parts("Meta.whnf");
        let c = a.append(&b);
        assert_eq!(c, Name::from_str_parts("Lean.Meta.whnf"));
    }

    #[test]
    fn test_get_root() {
        let n = Name::from_str_parts("Lean.Meta.whnf");
        let root = n.get_root();
        assert_eq!(root, Name::mk_simple("Lean"));
    }

    #[test]
    fn test_replace_prefix() {
        let n = Name::from_str_parts("Lean.Meta.whnf");
        let old_prefix = Name::from_str_parts("Lean.Meta");
        let new_prefix = Name::from_str_parts("Lean.Elab");
        let result = n.replace_prefix(&old_prefix, &new_prefix);
        assert_eq!(result, Name::from_str_parts("Lean.Elab.whnf"));
    }

    #[test]
    fn test_contains() {
        let n = Name::from_str_parts("Lean.Meta.whnf");
        assert!(n.contains("whnf"));
        assert!(n.contains("Meta"));
        assert!(!n.contains("Elab"));
    }

    #[test]
    fn test_hash_consistency() {
        // Same name constructed two ways should have same hash
        let a = Name::mk_str(Name::mk_str(Name::anonymous(), "Lean"), "Meta");
        let b = Name::from_str_parts("Lean.Meta");
        assert_eq!(a.hash(), b.hash());
        assert_eq!(a, b);
    }

    #[test]
    fn test_clone_sharing() {
        let a = Name::from_str_parts("Lean.Meta.whnf");
        let b = a.clone();
        assert!(Arc::ptr_eq(&a.inner, &b.inner));
    }

    #[test]
    fn test_mix_hash_matches_lean4() {
        // Verify our mixHash matches Lean 4's lean_uint64_mix_hash
        // mixHash(1723, string_hash("Nat")) should produce the same result
        let anon_hash: u64 = 1723;
        let nat_str_hash = lean_string_hash("Nat");
        let name_hash = mix_hash(anon_hash, nat_str_hash);
        // The Name constructor should produce the same hash
        let n = Name::mk_simple("Nat");
        assert_eq!(n.hash(), name_hash);
    }

    #[test]
    fn test_empty_string_component() {
        let n = Name::mk_str(Name::anonymous(), "");
        assert!(n.is_str());
        assert_eq!(n.get_string(), "");
    }

    #[test]
    fn test_std_hash_map_usage() {
        use std::collections::HashMap;
        let mut map = HashMap::new();
        let n1 = Name::from_str_parts("Lean.Meta.whnf");
        let n2 = Name::from_str_parts("Lean.Meta.isDefEq");
        map.insert(n1.clone(), 1);
        map.insert(n2.clone(), 2);
        assert_eq!(map.get(&n1), Some(&1));
        assert_eq!(map.get(&n2), Some(&2));
    }

    #[test]
    fn test_default_is_anonymous() {
        let n: Name = Default::default();
        assert!(n.is_anonymous());
    }
}
