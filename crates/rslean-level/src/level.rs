use rslean_name::Name;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::sync::Arc;

/// Universe levels in Lean 4's type theory.
///
/// Lean 4 uses a cumulative universe hierarchy:
///   Prop : Sort 0, Type u : Sort (u+1)
///
/// Levels are: Zero, Succ, Max, IMax, Param, MVar
#[derive(Clone)]
pub struct Level {
    inner: Arc<LevelInner>,
}

struct LevelInner {
    kind: LevelKind,
    data: LevelData,
}

/// Cached metadata for a level, matching Lean 4's packed uint64 layout.
#[derive(Clone, Copy)]
struct LevelData {
    hash: u32,
    has_mvar: bool,
    has_param: bool,
    depth: u32, // max 16777215
}

#[derive(Clone)]
enum LevelKind {
    Zero,
    Succ(Level),
    Max(Level, Level),
    IMax(Level, Level),
    Param(Name),
    MVar(Name),
}

impl Level {
    // --- Constructors ---

    pub fn zero() -> Self {
        Level {
            inner: Arc::new(LevelInner {
                kind: LevelKind::Zero,
                data: LevelData {
                    hash: 2221,
                    has_mvar: false,
                    has_param: false,
                    depth: 0,
                },
            }),
        }
    }

    pub fn succ(l: Level) -> Self {
        let data = LevelData {
            hash: mix_hash_u32(l.data().hash, 2243),
            has_mvar: l.has_mvar(),
            has_param: l.has_param(),
            depth: l.data().depth + 1,
        };
        Level {
            inner: Arc::new(LevelInner {
                kind: LevelKind::Succ(l),
                data,
            }),
        }
    }

    /// Raw max constructor (no normalization).
    fn max_core(l1: Level, l2: Level) -> Self {
        let data = LevelData {
            hash: mix_hash_u32(l1.data().hash, mix_hash_u32(l2.data().hash, 2251)),
            has_mvar: l1.has_mvar() || l2.has_mvar(),
            has_param: l1.has_param() || l2.has_param(),
            depth: std::cmp::max(l1.data().depth, l2.data().depth) + 1,
        };
        Level {
            inner: Arc::new(LevelInner {
                kind: LevelKind::Max(l1, l2),
                data,
            }),
        }
    }

    /// Raw imax constructor (no normalization).
    fn imax_core(l1: Level, l2: Level) -> Self {
        let data = LevelData {
            hash: mix_hash_u32(l1.data().hash, mix_hash_u32(l2.data().hash, 2267)),
            has_mvar: l1.has_mvar() || l2.has_mvar(),
            has_param: l1.has_param() || l2.has_param(),
            depth: std::cmp::max(l1.data().depth, l2.data().depth) + 1,
        };
        Level {
            inner: Arc::new(LevelInner {
                kind: LevelKind::IMax(l1, l2),
                data,
            }),
        }
    }

    /// Smart `max` constructor with simplifications.
    pub fn max(l1: Level, l2: Level) -> Self {
        if l1.is_explicit() && l2.is_explicit() {
            return if l1.depth() >= l2.depth() { l1 } else { l2 };
        }
        if l1 == l2 {
            return l1;
        }
        if l1.is_zero() {
            return l2;
        }
        if l2.is_zero() {
            return l1;
        }
        // l2 == max(l1, _) or max(_, l1) => max(l1, l2) == l2
        if let LevelKind::Max(ref a, ref b) = l2.inner.kind {
            if a == &l1 || b == &l1 {
                return l2;
            }
        }
        if let LevelKind::Max(ref a, ref b) = l1.inner.kind {
            if a == &l2 || b == &l2 {
                return l1;
            }
        }
        let (base1, off1) = l1.to_offset();
        let (base2, off2) = l2.to_offset();
        if base1 == base2 {
            return if off1 > off2 { l1 } else { l2 };
        }
        Level::max_core(l1, l2)
    }

    /// Smart `imax` constructor with simplifications.
    pub fn imax(l1: Level, l2: Level) -> Self {
        if l2.is_not_zero() {
            return Level::max(l1, l2);
        }
        if l2.is_zero() {
            return l2;
        } // imax u 0 = 0
        if l1.is_zero() || l1.is_one() {
            return l2;
        } // imax 0 u = imax 1 u = u
        if l1 == l2 {
            return l1;
        } // imax u u = u
        Level::imax_core(l1, l2)
    }

    pub fn param(n: Name) -> Self {
        let hash = mix_hash_u32(n.hash() as u32, 2237);
        Level {
            inner: Arc::new(LevelInner {
                kind: LevelKind::Param(n),
                data: LevelData {
                    hash,
                    has_mvar: false,
                    has_param: true,
                    depth: 0,
                },
            }),
        }
    }

    pub fn mvar(n: Name) -> Self {
        let hash = mix_hash_u32(n.hash() as u32, 2239);
        Level {
            inner: Arc::new(LevelInner {
                kind: LevelKind::MVar(n),
                data: LevelData {
                    hash,
                    has_mvar: true,
                    has_param: false,
                    depth: 0,
                },
            }),
        }
    }

    /// Convenience: `Type 0` = `Sort 1` = `Sort (succ zero)`
    pub fn one() -> Self {
        Level::succ(Level::zero())
    }

    /// Build `succ^k(l)`
    pub fn succ_n(mut l: Level, k: u32) -> Self {
        for _ in 0..k {
            l = Level::succ(l);
        }
        l
    }

    // --- Accessors ---

    fn data(&self) -> &LevelData {
        &self.inner.data
    }

    #[inline]
    pub fn hash(&self) -> u32 {
        self.inner.data.hash
    }

    #[inline]
    pub fn depth(&self) -> u32 {
        self.inner.data.depth
    }

    #[inline]
    pub fn has_mvar(&self) -> bool {
        self.inner.data.has_mvar
    }

    #[inline]
    pub fn has_param(&self) -> bool {
        self.inner.data.has_param
    }

    #[inline]
    pub fn is_zero(&self) -> bool {
        matches!(self.inner.kind, LevelKind::Zero)
    }

    #[inline]
    pub fn is_succ(&self) -> bool {
        matches!(self.inner.kind, LevelKind::Succ(_))
    }

    #[inline]
    pub fn is_max(&self) -> bool {
        matches!(self.inner.kind, LevelKind::Max(_, _))
    }

    #[inline]
    pub fn is_imax(&self) -> bool {
        matches!(self.inner.kind, LevelKind::IMax(_, _))
    }

    #[inline]
    pub fn is_param(&self) -> bool {
        matches!(self.inner.kind, LevelKind::Param(_))
    }

    #[inline]
    pub fn is_mvar(&self) -> bool {
        matches!(self.inner.kind, LevelKind::MVar(_))
    }

    pub fn is_one(&self) -> bool {
        if let LevelKind::Succ(ref inner) = self.inner.kind {
            inner.is_zero()
        } else {
            false
        }
    }

    /// A level is "explicit" if it's `succ^n(zero)` for some n.
    pub fn is_explicit(&self) -> bool {
        match &self.inner.kind {
            LevelKind::Zero => true,
            LevelKind::Succ(l) => l.is_explicit(),
            _ => false,
        }
    }

    /// Convert an explicit level to its integer value.
    pub fn to_explicit(&self) -> Option<u32> {
        if self.is_explicit() {
            Some(self.to_offset().1)
        } else {
            None
        }
    }

    /// Decompose `succ^k(l)` into `(l, k)`.
    pub fn to_offset(&self) -> (Level, u32) {
        let mut l = self.clone();
        let mut k = 0u32;
        while let LevelKind::Succ(ref inner) = l.inner.kind {
            let next = inner.clone();
            k += 1;
            l = next;
        }
        (l, k)
    }

    /// Get the inner level of `succ(l)`. Panics if not Succ.
    pub fn succ_of(&self) -> &Level {
        match &self.inner.kind {
            LevelKind::Succ(l) => l,
            _ => panic!("Level.succ_of: not a succ"),
        }
    }

    pub fn max_lhs(&self) -> &Level {
        match &self.inner.kind {
            LevelKind::Max(l, _) => l,
            _ => panic!("Level.max_lhs: not a max"),
        }
    }

    pub fn max_rhs(&self) -> &Level {
        match &self.inner.kind {
            LevelKind::Max(_, r) => r,
            _ => panic!("Level.max_rhs: not a max"),
        }
    }

    pub fn imax_lhs(&self) -> &Level {
        match &self.inner.kind {
            LevelKind::IMax(l, _) => l,
            _ => panic!("Level.imax_lhs: not an imax"),
        }
    }

    pub fn imax_rhs(&self) -> &Level {
        match &self.inner.kind {
            LevelKind::IMax(_, r) => r,
            _ => panic!("Level.imax_rhs: not an imax"),
        }
    }

    /// Get lhs of max or imax.
    pub fn lhs(&self) -> &Level {
        match &self.inner.kind {
            LevelKind::Max(l, _) | LevelKind::IMax(l, _) => l,
            _ => panic!("Level.lhs: not max/imax"),
        }
    }

    /// Get rhs of max or imax.
    pub fn rhs(&self) -> &Level {
        match &self.inner.kind {
            LevelKind::Max(_, r) | LevelKind::IMax(_, r) => r,
            _ => panic!("Level.rhs: not max/imax"),
        }
    }

    pub fn param_name(&self) -> &Name {
        match &self.inner.kind {
            LevelKind::Param(n) => n,
            _ => panic!("Level.param_name: not a param"),
        }
    }

    pub fn mvar_name(&self) -> &Name {
        match &self.inner.kind {
            LevelKind::MVar(n) => n,
            _ => panic!("Level.mvar_name: not an mvar"),
        }
    }

    pub fn level_id(&self) -> &Name {
        match &self.inner.kind {
            LevelKind::Param(n) | LevelKind::MVar(n) => n,
            _ => panic!("Level.level_id: not param/mvar"),
        }
    }

    // --- Predicates ---

    /// True if `l` is definitely not zero for any parameter assignment.
    pub fn is_not_zero(&self) -> bool {
        match &self.inner.kind {
            LevelKind::Zero | LevelKind::Param(_) | LevelKind::MVar(_) => false,
            LevelKind::Succ(_) => true,
            LevelKind::Max(l, r) => l.is_not_zero() || r.is_not_zero(),
            LevelKind::IMax(_, r) => r.is_not_zero(),
        }
    }

    // --- Operations ---

    /// Instantiate universe parameters: replace `Param(ps[i])` with `ls[i]`.
    pub fn instantiate(&self, ps: &[Name], ls: &[Level]) -> Level {
        self.replace(&|l| {
            if !l.has_param() {
                return Some(l.clone());
            }
            if let LevelKind::Param(ref n) = l.inner.kind {
                for (p, replacement) in ps.iter().zip(ls.iter()) {
                    if n == p {
                        return Some(replacement.clone());
                    }
                }
            }
            None
        })
    }

    /// Apply a function to each sub-level; if it returns Some, use that as replacement.
    pub fn replace(&self, f: &dyn Fn(&Level) -> Option<Level>) -> Level {
        if let Some(r) = f(self) {
            return r;
        }
        match &self.inner.kind {
            LevelKind::Zero | LevelKind::Param(_) | LevelKind::MVar(_) => self.clone(),
            LevelKind::Succ(l) => {
                let new_l = l.replace(f);
                if Arc::ptr_eq(&l.inner, &new_l.inner) {
                    self.clone()
                } else {
                    Level::succ(new_l)
                }
            }
            LevelKind::Max(l1, l2) => {
                let new_l1 = l1.replace(f);
                let new_l2 = l2.replace(f);
                if Arc::ptr_eq(&l1.inner, &new_l1.inner) && Arc::ptr_eq(&l2.inner, &new_l2.inner) {
                    self.clone()
                } else {
                    Level::max(new_l1, new_l2)
                }
            }
            LevelKind::IMax(l1, l2) => {
                let new_l1 = l1.replace(f);
                let new_l2 = l2.replace(f);
                if Arc::ptr_eq(&l1.inner, &new_l1.inner) && Arc::ptr_eq(&l2.inner, &new_l2.inner) {
                    self.clone()
                } else {
                    Level::imax(new_l1, new_l2)
                }
            }
        }
    }

    /// Visit each sub-level. Return false from callback to stop descent.
    pub fn for_each(&self, f: &mut dyn FnMut(&Level) -> bool) {
        if !f(self) {
            return;
        }
        match &self.inner.kind {
            LevelKind::Zero | LevelKind::Param(_) | LevelKind::MVar(_) => {}
            LevelKind::Succ(l) => l.for_each(f),
            LevelKind::Max(l1, l2) | LevelKind::IMax(l1, l2) => {
                l1.for_each(f);
                l2.for_each(f);
            }
        }
    }

    /// Check if `u` occurs anywhere in `self`.
    pub fn occurs(&self, u: &Level) -> bool {
        let mut found = false;
        self.for_each(&mut |l| {
            if found {
                return false;
            }
            if l == u {
                found = true;
                return false;
            }
            true
        });
        found
    }

    /// Find a universe parameter not in the given list.
    pub fn get_undef_param(&self, lparams: &[Name]) -> Option<Name> {
        let mut result = None;
        self.for_each(&mut |l| {
            if result.is_some() || !l.has_param() {
                return false;
            }
            if let LevelKind::Param(ref n) = l.inner.kind {
                if !lparams.contains(n) {
                    result = Some(n.clone());
                }
            }
            true
        });
        result
    }

    /// Normalize a level expression.
    pub fn normalize(&self) -> Level {
        let (base, offset) = self.to_offset();
        match &base.inner.kind {
            LevelKind::Succ(_) => unreachable!(),
            LevelKind::Zero | LevelKind::Param(_) | LevelKind::MVar(_) => self.clone(),
            LevelKind::IMax(l1, l2) => {
                let nl1 = l1.normalize();
                let nl2 = l2.normalize();
                Level::succ_n(Level::imax(nl1, nl2), offset)
            }
            LevelKind::Max(_, _) => {
                let mut args = Vec::new();
                push_max_args(&base, &mut args);
                let mut normalized: Vec<Level> = Vec::new();
                for a in &args {
                    let na = a.normalize();
                    push_max_args(&na, &mut normalized);
                }
                normalized.sort_by(norm_lt_cmp);
                // Deduplicate: keep highest offset for same base
                let mut rargs: Vec<Level> = Vec::new();
                let mut i = 0;
                if i < normalized.len() && normalized[i].is_explicit() {
                    while i + 1 < normalized.len() && normalized[i + 1].is_explicit() {
                        i += 1;
                    }
                    let k = normalized[i].to_offset().1;
                    let mut j = i + 1;
                    while j < normalized.len() {
                        if normalized[j].to_offset().1 >= k {
                            break;
                        }
                        j += 1;
                    }
                    if j < normalized.len() {
                        i += 1; // explicit was subsumed
                    }
                }
                if i < normalized.len() {
                    rargs.push(normalized[i].clone());
                    let mut prev = normalized[i].to_offset();
                    i += 1;
                    while i < normalized.len() {
                        let curr = normalized[i].to_offset();
                        if prev.0 == curr.0 {
                            if prev.1 < curr.1 {
                                prev = curr.clone();
                                rargs.pop();
                                rargs.push(normalized[i].clone());
                            }
                        } else {
                            prev = curr;
                            rargs.push(normalized[i].clone());
                        }
                        i += 1;
                    }
                }
                // Add offset back and fold into max
                for a in rargs.iter_mut() {
                    *a = Level::succ_n(a.clone(), offset);
                }
                fold_max(&rargs)
            }
        }
    }

    /// Check if two levels are equivalent (after normalization).
    pub fn is_equivalent(&self, other: &Level) -> bool {
        self == other || self.normalize() == other.normalize()
    }

    /// Check if `self >= other` for all parameter assignments.
    pub fn is_geq(&self, other: &Level) -> bool {
        is_geq_core(&self.normalize(), &other.normalize())
    }

    /// Kind discriminant for ordering.
    pub fn kind_num(&self) -> u8 {
        match &self.inner.kind {
            LevelKind::Zero => 0,
            LevelKind::Succ(_) => 1,
            LevelKind::Max(_, _) => 2,
            LevelKind::IMax(_, _) => 3,
            LevelKind::Param(_) => 4,
            LevelKind::MVar(_) => 5,
        }
    }
}

// --- Helper functions ---

/// Simple hash mixing for u32 (not the same as Lean's 64-bit mix_hash).
fn mix_hash_u32(a: u32, b: u32) -> u32 {
    // Use a simple combiner that matches Lean's approach for level hashing
    a.wrapping_mul(31).wrapping_add(b)
}

fn push_max_args(l: &Level, args: &mut Vec<Level>) {
    if let LevelKind::Max(ref a, ref b) = l.inner.kind {
        push_max_args(a, args);
        push_max_args(b, args);
    } else {
        args.push(l.clone());
    }
}

fn fold_max(args: &[Level]) -> Level {
    assert!(!args.is_empty());
    if args.len() == 1 {
        return args[0].clone();
    }
    let n = args.len();
    let mut r = Level::max(args[n - 2].clone(), args[n - 1].clone());
    for i in (0..n - 2).rev() {
        r = Level::max(args[i].clone(), r);
    }
    r
}

fn norm_lt_cmp(a: &Level, b: &Level) -> std::cmp::Ordering {
    if is_norm_lt(a, b) {
        std::cmp::Ordering::Less
    } else if is_norm_lt(b, a) {
        std::cmp::Ordering::Greater
    } else {
        std::cmp::Ordering::Equal
    }
}

fn is_norm_lt(a: &Level, b: &Level) -> bool {
    if Arc::ptr_eq(&a.inner, &b.inner) {
        return false;
    }
    let (l1, off1) = a.to_offset();
    let (l2, off2) = b.to_offset();
    if l1 != l2 {
        if l1.kind_num() != l2.kind_num() {
            return l1.kind_num() < l2.kind_num();
        }
        match (&l1.inner.kind, &l2.inner.kind) {
            (LevelKind::Zero, LevelKind::Zero) | (LevelKind::Succ(_), LevelKind::Succ(_)) => {
                unreachable!()
            }
            (LevelKind::Param(n1), LevelKind::Param(n2))
            | (LevelKind::MVar(n1), LevelKind::MVar(n2)) => n1 < n2,
            (LevelKind::Max(a1, a2), LevelKind::Max(b1, b2))
            | (LevelKind::IMax(a1, a2), LevelKind::IMax(b1, b2)) => {
                if a1 != b1 {
                    is_norm_lt(a1, b1)
                } else {
                    is_norm_lt(a2, b2)
                }
            }
            _ => unreachable!(),
        }
    } else {
        off1 < off2
    }
}

fn is_geq_core(l1: &Level, l2: &Level) -> bool {
    if l1 == l2 || l2.is_zero() {
        return true;
    }
    if let LevelKind::Max(ref a, ref b) = l2.inner.kind {
        return is_geq_core(l1, a) && is_geq_core(l1, b);
    }
    if let LevelKind::Max(ref a, ref b) = l1.inner.kind {
        if is_geq_core(a, l2) || is_geq_core(b, l2) {
            return true;
        }
    }
    if let LevelKind::IMax(ref a, ref b) = l2.inner.kind {
        return is_geq_core(l1, a) && is_geq_core(l1, b);
    }
    if let LevelKind::IMax(_, ref b) = l1.inner.kind {
        return is_geq_core(b, l2);
    }
    let (base1, off1) = l1.to_offset();
    let (base2, off2) = l2.to_offset();
    if base1 == base2 || base2.is_zero() {
        return off1 >= off2;
    }
    if off1 == off2 && off1 > 0 {
        return is_geq_core(&base1, &base2);
    }
    false
}

// --- Trait implementations ---

impl PartialEq for Level {
    fn eq(&self, other: &Self) -> bool {
        if Arc::ptr_eq(&self.inner, &other.inner) {
            return true;
        }
        if self.hash() != other.hash() {
            return false;
        }
        if self.kind_num() != other.kind_num() {
            return false;
        }
        match (&self.inner.kind, &other.inner.kind) {
            (LevelKind::Zero, LevelKind::Zero) => true,
            (LevelKind::Param(n1), LevelKind::Param(n2)) => n1 == n2,
            (LevelKind::MVar(n1), LevelKind::MVar(n2)) => n1 == n2,
            (LevelKind::Succ(a), LevelKind::Succ(b)) => a == b,
            (LevelKind::Max(a1, a2), LevelKind::Max(b1, b2)) => a1 == b1 && a2 == b2,
            (LevelKind::IMax(a1, a2), LevelKind::IMax(b1, b2)) => a1 == b1 && a2 == b2,
            _ => false,
        }
    }
}

impl Eq for Level {}

impl std::hash::Hash for Level {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        state.write_u32(self.inner.data.hash);
    }
}

impl fmt::Display for Level {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        display_level(f, self)
    }
}

impl fmt::Debug for Level {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Level({})", self)
    }
}

fn display_level(f: &mut fmt::Formatter<'_>, l: &Level) -> fmt::Result {
    if l.is_explicit() {
        write!(f, "{}", l.depth())
    } else {
        match &l.inner.kind {
            LevelKind::Zero => unreachable!(),
            LevelKind::Param(n) => write!(f, "{}", n),
            LevelKind::MVar(n) => write!(f, "?{}", n),
            LevelKind::Succ(inner) => {
                write!(f, "succ ")?;
                display_child(f, inner)
            }
            LevelKind::Max(a, b) => {
                write!(f, "max ")?;
                display_child(f, a)?;
                write!(f, " ")?;
                display_child(f, b)
            }
            LevelKind::IMax(a, b) => {
                write!(f, "imax ")?;
                display_child(f, a)?;
                write!(f, " ")?;
                display_child(f, b)
            }
        }
    }
}

fn display_child(f: &mut fmt::Formatter<'_>, l: &Level) -> fmt::Result {
    if l.is_explicit() || l.is_param() || l.is_mvar() {
        display_level(f, l)
    } else {
        write!(f, "(")?;
        display_level(f, l)?;
        write!(f, ")")
    }
}

#[derive(Serialize, Deserialize)]
enum LevelRepr {
    Zero,
    Succ(Box<LevelRepr>),
    Max(Box<LevelRepr>, Box<LevelRepr>),
    IMax(Box<LevelRepr>, Box<LevelRepr>),
    Param(Name),
    MVar(Name),
}

impl LevelRepr {
    fn from_level(l: &Level) -> Self {
        match &l.inner.kind {
            LevelKind::Zero => LevelRepr::Zero,
            LevelKind::Succ(inner) => LevelRepr::Succ(Box::new(LevelRepr::from_level(inner))),
            LevelKind::Max(a, b) => LevelRepr::Max(
                Box::new(LevelRepr::from_level(a)),
                Box::new(LevelRepr::from_level(b)),
            ),
            LevelKind::IMax(a, b) => LevelRepr::IMax(
                Box::new(LevelRepr::from_level(a)),
                Box::new(LevelRepr::from_level(b)),
            ),
            LevelKind::Param(n) => LevelRepr::Param(n.clone()),
            LevelKind::MVar(n) => LevelRepr::MVar(n.clone()),
        }
    }

    fn into_level(self) -> Level {
        match self {
            LevelRepr::Zero => Level::zero(),
            LevelRepr::Succ(inner) => Level::succ(inner.into_level()),
            LevelRepr::Max(a, b) => Level::max_core(a.into_level(), b.into_level()),
            LevelRepr::IMax(a, b) => Level::imax_core(a.into_level(), b.into_level()),
            LevelRepr::Param(n) => Level::param(n),
            LevelRepr::MVar(n) => Level::mvar(n),
        }
    }
}

impl Serialize for Level {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        LevelRepr::from_level(self).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Level {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        LevelRepr::deserialize(deserializer).map(|r| r.into_level())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zero() {
        let z = Level::zero();
        assert!(z.is_zero());
        assert!(z.is_explicit());
        assert_eq!(z.to_explicit(), Some(0));
        assert_eq!(z.depth(), 0);
        assert!(!z.has_mvar());
        assert!(!z.has_param());
        assert_eq!(z.to_string(), "0");
    }

    #[test]
    fn test_succ() {
        let one = Level::one();
        assert!(one.is_succ());
        assert!(one.is_explicit());
        assert_eq!(one.to_explicit(), Some(1));
        assert!(one.is_one());
        assert!(one.succ_of().is_zero());
        assert_eq!(one.to_string(), "1");

        let two = Level::succ(one.clone());
        assert_eq!(two.to_explicit(), Some(2));
        assert_eq!(two.to_string(), "2");
    }

    #[test]
    fn test_param() {
        let u = Level::param(Name::mk_simple("u"));
        assert!(u.is_param());
        assert!(!u.is_explicit());
        assert!(u.has_param());
        assert!(!u.has_mvar());
        assert_eq!(u.param_name(), &Name::mk_simple("u"));
        assert_eq!(u.to_string(), "u");
    }

    #[test]
    fn test_mvar() {
        let m = Level::mvar(Name::mk_simple("m"));
        assert!(m.is_mvar());
        assert!(m.has_mvar());
        assert!(!m.has_param());
        assert_eq!(m.to_string(), "?m");
    }

    #[test]
    fn test_max_simplification() {
        let z = Level::zero();
        let one = Level::one();
        let u = Level::param(Name::mk_simple("u"));

        // max 0 u = u
        assert_eq!(Level::max(z.clone(), u.clone()), u.clone());
        // max u 0 = u
        assert_eq!(Level::max(u.clone(), z.clone()), u.clone());
        // max u u = u
        assert_eq!(Level::max(u.clone(), u.clone()), u.clone());
        // max 0 1 = 1
        assert_eq!(Level::max(z.clone(), one.clone()), one);
    }

    #[test]
    fn test_imax_simplification() {
        let z = Level::zero();
        let u = Level::param(Name::mk_simple("u"));
        let v = Level::param(Name::mk_simple("v"));

        // imax u 0 = 0
        assert!(Level::imax(u.clone(), z.clone()).is_zero());
        // imax 0 v = v
        assert_eq!(Level::imax(z.clone(), v.clone()), v.clone());
        // imax u u = u
        assert_eq!(Level::imax(u.clone(), u.clone()), u);
    }

    #[test]
    fn test_to_offset() {
        let u = Level::param(Name::mk_simple("u"));
        let su = Level::succ(u.clone());
        let ssu = Level::succ(su.clone());
        let (base, offset) = ssu.to_offset();
        assert_eq!(base, u);
        assert_eq!(offset, 2);
    }

    #[test]
    fn test_equality() {
        let u = Level::param(Name::mk_simple("u"));
        let v = Level::param(Name::mk_simple("v"));
        let u2 = Level::param(Name::mk_simple("u"));
        assert_eq!(u, u2);
        assert_ne!(u, v);
    }

    #[test]
    fn test_instantiate() {
        let u = Level::param(Name::mk_simple("u"));
        let v = Level::param(Name::mk_simple("v"));
        let max_uv = Level::max(u.clone(), v.clone());

        let ps = vec![Name::mk_simple("u")];
        let ls = vec![Level::one()];
        let result = max_uv.instantiate(&ps, &ls);
        // max 1 v => should simplify
        assert_eq!(result, Level::max(Level::one(), v.clone()));
    }

    #[test]
    fn test_is_not_zero() {
        let z = Level::zero();
        let one = Level::one();
        let u = Level::param(Name::mk_simple("u"));
        assert!(!z.is_not_zero());
        assert!(one.is_not_zero());
        assert!(!u.is_not_zero());
        assert!(Level::succ(u.clone()).is_not_zero());
        assert!(Level::max(one.clone(), u.clone()).is_not_zero());
    }

    #[test]
    fn test_is_equivalent() {
        let u = Level::param(Name::mk_simple("u"));
        let max_u_u = Level::max_core(u.clone(), u.clone());
        assert!(u.is_equivalent(&max_u_u));
    }

    #[test]
    fn test_is_geq() {
        let z = Level::zero();
        let one = Level::one();
        let u = Level::param(Name::mk_simple("u"));
        let su = Level::succ(u.clone());

        assert!(one.is_geq(&z));
        assert!(su.is_geq(&u));
        assert!(!u.is_geq(&su));
    }

    #[test]
    fn test_occurs() {
        let u = Level::param(Name::mk_simple("u"));
        let v = Level::param(Name::mk_simple("v"));
        let max = Level::max(u.clone(), v.clone());
        assert!(max.occurs(&u));
        assert!(max.occurs(&v));
        assert!(!max.occurs(&Level::zero()));
    }

    #[test]
    fn test_get_undef_param() {
        let u = Level::param(Name::mk_simple("u"));
        let v = Level::param(Name::mk_simple("v"));
        let max = Level::max(u.clone(), v.clone());
        let params = vec![Name::mk_simple("u")];
        let undef = max.get_undef_param(&params);
        assert_eq!(undef, Some(Name::mk_simple("v")));
    }

    #[test]
    fn test_normalize_max() {
        let u = Level::param(Name::mk_simple("u"));
        let su = Level::succ(u.clone());
        // max u (succ u) should normalize to succ u
        let m = Level::max_core(u.clone(), su.clone());
        let n = m.normalize();
        assert_eq!(n, su);
    }

    #[test]
    fn test_display() {
        let u = Level::param(Name::mk_simple("u"));
        let v = Level::param(Name::mk_simple("v"));
        let max = Level::max(u.clone(), v.clone());
        assert_eq!(max.to_string(), "max u v");

        let su = Level::succ(u.clone());
        assert_eq!(su.to_string(), "succ u");
    }
}
