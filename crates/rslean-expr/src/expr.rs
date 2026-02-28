use crate::Literal;
use rslean_level::Level;
use rslean_name::Name;
use std::fmt;
use std::sync::Arc;

/// Binder annotation info.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BinderInfo {
    Default,
    Implicit,
    StrictImplicit,
    InstImplicit,
}

/// Metadata key-value map (simplified — we use a list of (Name, value) pairs).
pub type MData = Vec<(Name, MDataValue)>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MDataValue {
    Bool(bool),
    Name(Name),
    Nat(u64),
    String(String),
}

/// Expression AST — the core type of Lean 4's type theory.
///
/// 12 constructors matching Lean 4's `Expr` inductive type.
/// Each `Expr` caches metadata (hash, flags) for efficient comparison.
#[derive(Clone)]
pub struct Expr {
    inner: Arc<ExprInner>,
}

struct ExprInner {
    kind: ExprKind,
    data: ExprData,
}

/// Cached metadata for an expression, matching Lean 4's packed 64-bit layout.
#[derive(Clone, Copy)]
struct ExprData {
    hash: u32,
    approx_depth: u8,
    has_fvar: bool,
    has_expr_mvar: bool,
    has_univ_mvar: bool,
    has_univ_param: bool,
    loose_bvar_range: u32, // 20 bits in Lean, we use u32
}

#[derive(Clone)]
pub enum ExprKind {
    BVar(u64),
    FVar(Name),
    MVar(Name),
    Sort(Level),
    Const(Name, Vec<Level>),
    App(Expr, Expr),
    Lam(Name, Expr, Expr, BinderInfo),
    ForallE(Name, Expr, Expr, BinderInfo),
    LetE(Name, Expr, Expr, Expr, bool), // name, type, value, body, nonDep
    Lit(Literal),
    MData(MData, Expr),
    Proj(Name, u64, Expr),
}

fn hash_u64(n: u64) -> u32 {
    let mut h = n as u32;
    h = h.wrapping_mul(2654435761);
    h ^= (n >> 32) as u32;
    h
}

fn mix(a: u32, b: u32) -> u32 {
    a.wrapping_mul(31).wrapping_add(b)
}

fn hash_levels(ls: &[Level]) -> u32 {
    let mut h = 0u32;
    for l in ls {
        h = mix(h, l.hash());
    }
    h
}

impl Expr {
    // --- Constructors ---

    pub fn bvar(idx: u64) -> Self {
        let data = ExprData {
            hash: mix(7, hash_u64(idx)),
            approx_depth: 0,
            has_fvar: false,
            has_expr_mvar: false,
            has_univ_mvar: false,
            has_univ_param: false,
            loose_bvar_range: (idx as u32).saturating_add(1),
        };
        Expr { inner: Arc::new(ExprInner { kind: ExprKind::BVar(idx), data }) }
    }

    pub fn fvar(id: Name) -> Self {
        let data = ExprData {
            hash: mix(11, id.hash() as u32),
            approx_depth: 0,
            has_fvar: true,
            has_expr_mvar: false,
            has_univ_mvar: false,
            has_univ_param: false,
            loose_bvar_range: 0,
        };
        Expr { inner: Arc::new(ExprInner { kind: ExprKind::FVar(id), data }) }
    }

    pub fn mvar(id: Name) -> Self {
        let data = ExprData {
            hash: mix(13, id.hash() as u32),
            approx_depth: 0,
            has_fvar: false,
            has_expr_mvar: true,
            has_univ_mvar: false,
            has_univ_param: false,
            loose_bvar_range: 0,
        };
        Expr { inner: Arc::new(ExprInner { kind: ExprKind::MVar(id), data }) }
    }

    pub fn sort(level: Level) -> Self {
        let data = ExprData {
            hash: mix(17, level.hash()),
            approx_depth: 0,
            has_fvar: false,
            has_expr_mvar: false,
            has_univ_mvar: level.has_mvar(),
            has_univ_param: level.has_param(),
            loose_bvar_range: 0,
        };
        Expr { inner: Arc::new(ExprInner { kind: ExprKind::Sort(level), data }) }
    }

    pub fn prop() -> Self {
        Expr::sort(Level::zero())
    }

    pub fn type_() -> Self {
        Expr::sort(Level::one())
    }

    pub fn const_(name: Name, levels: Vec<Level>) -> Self {
        let has_univ_mvar = levels.iter().any(|l| l.has_mvar());
        let has_univ_param = levels.iter().any(|l| l.has_param());
        let data = ExprData {
            hash: mix(mix(19, name.hash() as u32), hash_levels(&levels)),
            approx_depth: 0,
            has_fvar: false,
            has_expr_mvar: false,
            has_univ_mvar,
            has_univ_param,
            loose_bvar_range: 0,
        };
        Expr { inner: Arc::new(ExprInner { kind: ExprKind::Const(name, levels), data }) }
    }

    pub fn app(f: Expr, a: Expr) -> Self {
        let data = ExprData {
            hash: mix(mix(23, f.hash()), a.hash()),
            approx_depth: std::cmp::max(f.approx_depth(), a.approx_depth()).saturating_add(1),
            has_fvar: f.has_fvar() || a.has_fvar(),
            has_expr_mvar: f.has_expr_mvar() || a.has_expr_mvar(),
            has_univ_mvar: f.has_univ_mvar() || a.has_univ_mvar(),
            has_univ_param: f.has_univ_param() || a.has_univ_param(),
            loose_bvar_range: std::cmp::max(f.loose_bvar_range(), a.loose_bvar_range()),
        };
        Expr { inner: Arc::new(ExprInner { kind: ExprKind::App(f, a), data }) }
    }

    /// Build nested applications: `mk_app(f, [a1, a2, a3])` → `App(App(App(f, a1), a2), a3)`
    pub fn mk_app(f: Expr, args: &[Expr]) -> Self {
        let mut result = f;
        for arg in args {
            result = Expr::app(result, arg.clone());
        }
        result
    }

    pub fn lam(name: Name, ty: Expr, body: Expr, bi: BinderInfo) -> Self {
        let body_range = if body.loose_bvar_range() > 0 {
            body.loose_bvar_range() - 1
        } else {
            0
        };
        let data = ExprData {
            hash: mix(mix(mix(29, name.hash() as u32), ty.hash()), body.hash()),
            approx_depth: std::cmp::max(ty.approx_depth(), body.approx_depth()).saturating_add(1),
            has_fvar: ty.has_fvar() || body.has_fvar(),
            has_expr_mvar: ty.has_expr_mvar() || body.has_expr_mvar(),
            has_univ_mvar: ty.has_univ_mvar() || body.has_univ_mvar(),
            has_univ_param: ty.has_univ_param() || body.has_univ_param(),
            loose_bvar_range: std::cmp::max(ty.loose_bvar_range(), body_range),
        };
        Expr { inner: Arc::new(ExprInner { kind: ExprKind::Lam(name, ty, body, bi), data }) }
    }

    pub fn forall_e(name: Name, ty: Expr, body: Expr, bi: BinderInfo) -> Self {
        let body_range = if body.loose_bvar_range() > 0 {
            body.loose_bvar_range() - 1
        } else {
            0
        };
        let data = ExprData {
            hash: mix(mix(mix(31, name.hash() as u32), ty.hash()), body.hash()),
            approx_depth: std::cmp::max(ty.approx_depth(), body.approx_depth()).saturating_add(1),
            has_fvar: ty.has_fvar() || body.has_fvar(),
            has_expr_mvar: ty.has_expr_mvar() || body.has_expr_mvar(),
            has_univ_mvar: ty.has_univ_mvar() || body.has_univ_mvar(),
            has_univ_param: ty.has_univ_param() || body.has_univ_param(),
            loose_bvar_range: std::cmp::max(ty.loose_bvar_range(), body_range),
        };
        Expr { inner: Arc::new(ExprInner { kind: ExprKind::ForallE(name, ty, body, bi), data }) }
    }

    /// Non-dependent arrow: `A → B` = `ForallE("_", A, B, Default)` where B has no loose bvar 0.
    pub fn arrow(a: Expr, b: Expr) -> Self {
        Expr::forall_e(
            Name::anonymous(),
            a,
            b.lift_loose_bvars(0, 1),
            BinderInfo::Default,
        )
    }

    pub fn let_e(name: Name, ty: Expr, val: Expr, body: Expr, non_dep: bool) -> Self {
        let body_range = if body.loose_bvar_range() > 0 {
            body.loose_bvar_range() - 1
        } else {
            0
        };
        let data = ExprData {
            hash: mix(mix(mix(mix(37, name.hash() as u32), ty.hash()), val.hash()), body.hash()),
            approx_depth: std::cmp::max(
                std::cmp::max(ty.approx_depth(), val.approx_depth()),
                body.approx_depth(),
            ).saturating_add(1),
            has_fvar: ty.has_fvar() || val.has_fvar() || body.has_fvar(),
            has_expr_mvar: ty.has_expr_mvar() || val.has_expr_mvar() || body.has_expr_mvar(),
            has_univ_mvar: ty.has_univ_mvar() || val.has_univ_mvar() || body.has_univ_mvar(),
            has_univ_param: ty.has_univ_param() || val.has_univ_param() || body.has_univ_param(),
            loose_bvar_range: std::cmp::max(
                std::cmp::max(ty.loose_bvar_range(), val.loose_bvar_range()),
                body_range,
            ),
        };
        Expr { inner: Arc::new(ExprInner { kind: ExprKind::LetE(name, ty, val, body, non_dep), data }) }
    }

    pub fn lit(l: Literal) -> Self {
        let hash = match &l {
            Literal::Nat(n) => {
                let bytes = n.to_bytes_le();
                let mut h = 41u32;
                for b in bytes {
                    h = mix(h, b as u32);
                }
                h
            }
            Literal::Str(s) => {
                let mut h = 43u32;
                for b in s.bytes() {
                    h = mix(h, b as u32);
                }
                h
            }
        };
        let data = ExprData {
            hash,
            approx_depth: 0,
            has_fvar: false,
            has_expr_mvar: false,
            has_univ_mvar: false,
            has_univ_param: false,
            loose_bvar_range: 0,
        };
        Expr { inner: Arc::new(ExprInner { kind: ExprKind::Lit(l), data }) }
    }

    pub fn mdata(md: MData, e: Expr) -> Self {
        let data = e.data_ref().clone();
        let data = ExprData {
            hash: mix(47, data.hash),
            ..data
        };
        Expr { inner: Arc::new(ExprInner { kind: ExprKind::MData(md, e), data }) }
    }

    pub fn proj(sname: Name, idx: u64, e: Expr) -> Self {
        let data = ExprData {
            hash: mix(mix(mix(53, sname.hash() as u32), hash_u64(idx)), e.hash()),
            approx_depth: e.approx_depth().saturating_add(1),
            has_fvar: e.has_fvar(),
            has_expr_mvar: e.has_expr_mvar(),
            has_univ_mvar: e.has_univ_mvar(),
            has_univ_param: e.has_univ_param(),
            loose_bvar_range: e.loose_bvar_range(),
        };
        Expr { inner: Arc::new(ExprInner { kind: ExprKind::Proj(sname, idx, e), data }) }
    }

    // --- Accessors ---

    fn data_ref(&self) -> &ExprData {
        &self.inner.data
    }

    #[inline]
    pub fn hash(&self) -> u32 {
        self.inner.data.hash
    }

    #[inline]
    pub fn approx_depth(&self) -> u8 {
        self.inner.data.approx_depth
    }

    #[inline]
    pub fn has_fvar(&self) -> bool {
        self.inner.data.has_fvar
    }

    #[inline]
    pub fn has_expr_mvar(&self) -> bool {
        self.inner.data.has_expr_mvar
    }

    #[inline]
    pub fn has_univ_mvar(&self) -> bool {
        self.inner.data.has_univ_mvar
    }

    #[inline]
    pub fn has_univ_param(&self) -> bool {
        self.inner.data.has_univ_param
    }

    #[inline]
    pub fn has_mvar(&self) -> bool {
        self.has_expr_mvar() || self.has_univ_mvar()
    }

    #[inline]
    pub fn loose_bvar_range(&self) -> u32 {
        self.inner.data.loose_bvar_range
    }

    #[inline]
    pub fn has_loose_bvars(&self) -> bool {
        self.loose_bvar_range() > 0
    }

    pub fn kind(&self) -> &ExprKind {
        &self.inner.kind
    }

    // --- Kind tests ---

    pub fn is_bvar(&self) -> bool { matches!(self.inner.kind, ExprKind::BVar(_)) }
    pub fn is_fvar(&self) -> bool { matches!(self.inner.kind, ExprKind::FVar(_)) }
    pub fn is_mvar(&self) -> bool { matches!(self.inner.kind, ExprKind::MVar(_)) }
    pub fn is_sort(&self) -> bool { matches!(self.inner.kind, ExprKind::Sort(_)) }
    pub fn is_const(&self) -> bool { matches!(self.inner.kind, ExprKind::Const(_, _)) }
    pub fn is_app(&self) -> bool { matches!(self.inner.kind, ExprKind::App(_, _)) }
    pub fn is_lam(&self) -> bool { matches!(self.inner.kind, ExprKind::Lam(..)) }
    pub fn is_forall(&self) -> bool { matches!(self.inner.kind, ExprKind::ForallE(..)) }
    pub fn is_let(&self) -> bool { matches!(self.inner.kind, ExprKind::LetE(..)) }
    pub fn is_lit(&self) -> bool { matches!(self.inner.kind, ExprKind::Lit(_)) }
    pub fn is_mdata(&self) -> bool { matches!(self.inner.kind, ExprKind::MData(..)) }
    pub fn is_proj(&self) -> bool { matches!(self.inner.kind, ExprKind::Proj(..)) }
    pub fn is_binding(&self) -> bool { self.is_lam() || self.is_forall() }
    pub fn is_prop(&self) -> bool {
        matches!(&self.inner.kind, ExprKind::Sort(l) if l.is_zero())
    }

    // --- Destructors ---

    pub fn bvar_idx(&self) -> u64 {
        match &self.inner.kind { ExprKind::BVar(i) => *i, _ => panic!("not bvar") }
    }
    pub fn fvar_name(&self) -> &Name {
        match &self.inner.kind { ExprKind::FVar(n) => n, _ => panic!("not fvar") }
    }
    pub fn mvar_name(&self) -> &Name {
        match &self.inner.kind { ExprKind::MVar(n) => n, _ => panic!("not mvar") }
    }
    pub fn sort_level(&self) -> &Level {
        match &self.inner.kind { ExprKind::Sort(l) => l, _ => panic!("not sort") }
    }
    pub fn const_name(&self) -> &Name {
        match &self.inner.kind { ExprKind::Const(n, _) => n, _ => panic!("not const") }
    }
    pub fn const_levels(&self) -> &[Level] {
        match &self.inner.kind { ExprKind::Const(_, ls) => ls, _ => panic!("not const") }
    }
    pub fn app_fn(&self) -> &Expr {
        match &self.inner.kind { ExprKind::App(f, _) => f, _ => panic!("not app") }
    }
    pub fn app_arg(&self) -> &Expr {
        match &self.inner.kind { ExprKind::App(_, a) => a, _ => panic!("not app") }
    }
    pub fn binding_name(&self) -> &Name {
        match &self.inner.kind {
            ExprKind::Lam(n, ..) | ExprKind::ForallE(n, ..) => n,
            _ => panic!("not binding"),
        }
    }
    pub fn binding_domain(&self) -> &Expr {
        match &self.inner.kind {
            ExprKind::Lam(_, d, ..) | ExprKind::ForallE(_, d, ..) => d,
            _ => panic!("not binding"),
        }
    }
    pub fn binding_body(&self) -> &Expr {
        match &self.inner.kind {
            ExprKind::Lam(_, _, b, _) | ExprKind::ForallE(_, _, b, _) => b,
            _ => panic!("not binding"),
        }
    }
    pub fn binding_info(&self) -> BinderInfo {
        match &self.inner.kind {
            ExprKind::Lam(_, _, _, bi) | ExprKind::ForallE(_, _, _, bi) => *bi,
            _ => panic!("not binding"),
        }
    }
    pub fn let_name(&self) -> &Name {
        match &self.inner.kind { ExprKind::LetE(n, ..) => n, _ => panic!("not let") }
    }
    pub fn let_type(&self) -> &Expr {
        match &self.inner.kind { ExprKind::LetE(_, t, ..) => t, _ => panic!("not let") }
    }
    pub fn let_value(&self) -> &Expr {
        match &self.inner.kind { ExprKind::LetE(_, _, v, ..) => v, _ => panic!("not let") }
    }
    pub fn let_body(&self) -> &Expr {
        match &self.inner.kind { ExprKind::LetE(_, _, _, b, _) => b, _ => panic!("not let") }
    }
    pub fn let_nondep(&self) -> bool {
        match &self.inner.kind { ExprKind::LetE(_, _, _, _, nd) => *nd, _ => panic!("not let") }
    }
    pub fn lit_value(&self) -> &Literal {
        match &self.inner.kind { ExprKind::Lit(l) => l, _ => panic!("not lit") }
    }
    pub fn mdata_data(&self) -> &MData {
        match &self.inner.kind { ExprKind::MData(d, _) => d, _ => panic!("not mdata") }
    }
    pub fn mdata_expr(&self) -> &Expr {
        match &self.inner.kind { ExprKind::MData(_, e) => e, _ => panic!("not mdata") }
    }
    pub fn proj_sname(&self) -> &Name {
        match &self.inner.kind { ExprKind::Proj(s, ..) => s, _ => panic!("not proj") }
    }
    pub fn proj_idx(&self) -> u64 {
        match &self.inner.kind { ExprKind::Proj(_, i, _) => *i, _ => panic!("not proj") }
    }
    pub fn proj_expr(&self) -> &Expr {
        match &self.inner.kind { ExprKind::Proj(_, _, e) => e, _ => panic!("not proj") }
    }

    // --- App utilities ---

    /// Get the head function of `f a1 a2 ... an`, returns `f`.
    pub fn get_app_fn(&self) -> &Expr {
        let mut e = self;
        while let ExprKind::App(f, _) = &e.inner.kind {
            e = f;
        }
        e
    }

    /// Collect arguments: `f a1 a2 ... an` → pushes [a1, ..., an] and returns &f.
    pub fn get_app_args(&self, args: &mut Vec<Expr>) -> &Expr {
        let mut e = self;
        while let ExprKind::App(f, a) = &e.inner.kind {
            args.push(a.clone());
            e = f;
        }
        args.reverse();
        e
    }

    /// Count number of arguments.
    pub fn get_app_num_args(&self) -> usize {
        let mut e = self;
        let mut n = 0;
        while let ExprKind::App(f, _) = &e.inner.kind {
            n += 1;
            e = f;
        }
        n
    }

    // --- Substitution operations ---

    /// Replace loose BVar(i) with `subst[i]`.
    pub fn instantiate(&self, subst: &[Expr]) -> Expr {
        if subst.is_empty() || !self.has_loose_bvars() {
            return self.clone();
        }
        self.instantiate_core(subst, 0)
    }

    fn instantiate_core(&self, subst: &[Expr], offset: u64) -> Expr {
        if !self.has_loose_bvars() {
            return self.clone();
        }
        match &self.inner.kind {
            ExprKind::BVar(idx) => {
                if *idx >= offset && (*idx - offset) < subst.len() as u64 {
                    let s = &subst[(*idx - offset) as usize];
                    if offset > 0 { s.lift_loose_bvars(0, offset as u32) } else { s.clone() }
                } else {
                    self.clone()
                }
            }
            ExprKind::App(f, a) => {
                let nf = f.instantiate_core(subst, offset);
                let na = a.instantiate_core(subst, offset);
                Expr::app(nf, na)
            }
            ExprKind::Lam(n, d, b, bi) => {
                let nd = d.instantiate_core(subst, offset);
                let nb = b.instantiate_core(subst, offset + 1);
                Expr::lam(n.clone(), nd, nb, *bi)
            }
            ExprKind::ForallE(n, d, b, bi) => {
                let nd = d.instantiate_core(subst, offset);
                let nb = b.instantiate_core(subst, offset + 1);
                Expr::forall_e(n.clone(), nd, nb, *bi)
            }
            ExprKind::LetE(n, t, v, b, nd) => {
                let nt = t.instantiate_core(subst, offset);
                let nv = v.instantiate_core(subst, offset);
                let nb = b.instantiate_core(subst, offset + 1);
                Expr::let_e(n.clone(), nt, nv, nb, *nd)
            }
            ExprKind::MData(md, e) => {
                let ne = e.instantiate_core(subst, offset);
                Expr::mdata(md.clone(), ne)
            }
            ExprKind::Proj(sn, i, e) => {
                let ne = e.instantiate_core(subst, offset);
                Expr::proj(sn.clone(), *i, ne)
            }
            _ => self.clone(),
        }
    }

    /// Replace loose BVar(0) with a single expression.
    pub fn instantiate1(&self, s: &Expr) -> Expr {
        self.instantiate(&[s.clone()])
    }

    /// Reverse instantiate: subst[n-1] replaces BVar(0), subst[n-2] replaces BVar(1), etc.
    pub fn instantiate_rev(&self, subst: &[Expr]) -> Expr {
        if subst.is_empty() || !self.has_loose_bvars() {
            return self.clone();
        }
        let mut rev: Vec<Expr> = subst.to_vec();
        rev.reverse();
        self.instantiate(&rev)
    }

    /// Lift loose bound variables by `d`, starting from index `s`.
    /// BVar(i) where i >= s becomes BVar(i + d).
    pub fn lift_loose_bvars(&self, s: u32, d: u32) -> Expr {
        if d == 0 || !self.has_loose_bvars() {
            return self.clone();
        }
        self.lift_core(s as u64, d as u64, 0)
    }

    fn lift_core(&self, s: u64, d: u64, offset: u64) -> Expr {
        if !self.has_loose_bvars() {
            return self.clone();
        }
        match &self.inner.kind {
            ExprKind::BVar(idx) => {
                if *idx >= offset + s {
                    Expr::bvar(idx + d)
                } else {
                    self.clone()
                }
            }
            ExprKind::App(f, a) => {
                Expr::app(f.lift_core(s, d, offset), a.lift_core(s, d, offset))
            }
            ExprKind::Lam(n, dom, body, bi) => {
                Expr::lam(n.clone(), dom.lift_core(s, d, offset), body.lift_core(s, d, offset + 1), *bi)
            }
            ExprKind::ForallE(n, dom, body, bi) => {
                Expr::forall_e(n.clone(), dom.lift_core(s, d, offset), body.lift_core(s, d, offset + 1), *bi)
            }
            ExprKind::LetE(n, t, v, b, nd) => {
                Expr::let_e(
                    n.clone(),
                    t.lift_core(s, d, offset),
                    v.lift_core(s, d, offset),
                    b.lift_core(s, d, offset + 1),
                    *nd,
                )
            }
            ExprKind::MData(md, e) => Expr::mdata(md.clone(), e.lift_core(s, d, offset)),
            ExprKind::Proj(sn, i, e) => Expr::proj(sn.clone(), *i, e.lift_core(s, d, offset)),
            _ => self.clone(),
        }
    }

    /// Lower loose bound variables by `d` starting from `s`.
    pub fn lower_loose_bvars(&self, s: u32, d: u32) -> Expr {
        if d == 0 || !self.has_loose_bvars() {
            return self.clone();
        }
        self.lower_core(s as u64, d as u64, 0)
    }

    fn lower_core(&self, s: u64, d: u64, offset: u64) -> Expr {
        if !self.has_loose_bvars() {
            return self.clone();
        }
        match &self.inner.kind {
            ExprKind::BVar(idx) => {
                if *idx >= offset + s {
                    Expr::bvar(idx - d)
                } else {
                    self.clone()
                }
            }
            ExprKind::App(f, a) => {
                Expr::app(f.lower_core(s, d, offset), a.lower_core(s, d, offset))
            }
            ExprKind::Lam(n, dom, body, bi) => {
                Expr::lam(n.clone(), dom.lower_core(s, d, offset), body.lower_core(s, d, offset + 1), *bi)
            }
            ExprKind::ForallE(n, dom, body, bi) => {
                Expr::forall_e(n.clone(), dom.lower_core(s, d, offset), body.lower_core(s, d, offset + 1), *bi)
            }
            ExprKind::LetE(n, t, v, b, nd) => {
                Expr::let_e(
                    n.clone(),
                    t.lower_core(s, d, offset),
                    v.lower_core(s, d, offset),
                    b.lower_core(s, d, offset + 1),
                    *nd,
                )
            }
            ExprKind::MData(md, e) => Expr::mdata(md.clone(), e.lower_core(s, d, offset)),
            ExprKind::Proj(sn, i, e) => Expr::proj(sn.clone(), *i, e.lower_core(s, d, offset)),
            _ => self.clone(),
        }
    }

    /// Abstract: replace occurrences of `s[i]` with `BVar(i)`.
    pub fn abstract_(&self, s: &[Expr]) -> Expr {
        if s.is_empty() {
            return self.clone();
        }
        self.abstract_core(s, 0)
    }

    fn abstract_core(&self, s: &[Expr], offset: u64) -> Expr {
        for (i, x) in s.iter().enumerate() {
            if self == x {
                return Expr::bvar(i as u64 + offset);
            }
        }
        match &self.inner.kind {
            ExprKind::App(f, a) => {
                Expr::app(f.abstract_core(s, offset), a.abstract_core(s, offset))
            }
            ExprKind::Lam(n, d, b, bi) => {
                Expr::lam(n.clone(), d.abstract_core(s, offset), b.abstract_core(s, offset + 1), *bi)
            }
            ExprKind::ForallE(n, d, b, bi) => {
                Expr::forall_e(n.clone(), d.abstract_core(s, offset), b.abstract_core(s, offset + 1), *bi)
            }
            ExprKind::LetE(n, t, v, b, nd) => {
                Expr::let_e(
                    n.clone(),
                    t.abstract_core(s, offset),
                    v.abstract_core(s, offset),
                    b.abstract_core(s, offset + 1),
                    *nd,
                )
            }
            ExprKind::MData(md, e) => Expr::mdata(md.clone(), e.abstract_core(s, offset)),
            ExprKind::Proj(sn, i, e) => Expr::proj(sn.clone(), *i, e.abstract_core(s, offset)),
            _ => self.clone(),
        }
    }

    /// Instantiate universe level parameters in this expression.
    pub fn instantiate_level_params(&self, ps: &[Name], ls: &[Level]) -> Expr {
        if ps.is_empty() || (!self.has_univ_param()) {
            return self.clone();
        }
        self.replace(&|e| {
            if !e.has_univ_param() {
                return Some(e.clone());
            }
            match &e.inner.kind {
                ExprKind::Sort(l) => {
                    let nl = l.instantiate(ps, ls);
                    Some(Expr::sort(nl))
                }
                ExprKind::Const(n, lvls) => {
                    let new_lvls: Vec<Level> =
                        lvls.iter().map(|l| l.instantiate(ps, ls)).collect();
                    Some(Expr::const_(n.clone(), new_lvls))
                }
                _ => None,
            }
        })
    }

    /// Apply a transformation function to all sub-expressions.
    pub fn replace(&self, f: &dyn Fn(&Expr) -> Option<Expr>) -> Expr {
        if let Some(r) = f(self) {
            return r;
        }
        match &self.inner.kind {
            ExprKind::BVar(_) | ExprKind::FVar(_) | ExprKind::MVar(_)
            | ExprKind::Sort(_) | ExprKind::Const(_, _) | ExprKind::Lit(_) => self.clone(),
            ExprKind::App(fn_, arg) => {
                let nf = fn_.replace(f);
                let na = arg.replace(f);
                Expr::app(nf, na)
            }
            ExprKind::Lam(n, d, b, bi) => {
                Expr::lam(n.clone(), d.replace(f), b.replace(f), *bi)
            }
            ExprKind::ForallE(n, d, b, bi) => {
                Expr::forall_e(n.clone(), d.replace(f), b.replace(f), *bi)
            }
            ExprKind::LetE(n, t, v, b, nd) => {
                Expr::let_e(n.clone(), t.replace(f), v.replace(f), b.replace(f), *nd)
            }
            ExprKind::MData(md, e) => Expr::mdata(md.clone(), e.replace(f)),
            ExprKind::Proj(sn, i, e) => Expr::proj(sn.clone(), *i, e.replace(f)),
        }
    }

    /// Visit each sub-expression. Return false to stop descent.
    pub fn for_each(&self, f: &mut dyn FnMut(&Expr) -> bool) {
        if !f(self) {
            return;
        }
        match &self.inner.kind {
            ExprKind::BVar(_) | ExprKind::FVar(_) | ExprKind::MVar(_)
            | ExprKind::Sort(_) | ExprKind::Const(_, _) | ExprKind::Lit(_) => {}
            ExprKind::App(fn_, arg) => { fn_.for_each(f); arg.for_each(f); }
            ExprKind::Lam(_, d, b, _) | ExprKind::ForallE(_, d, b, _) => {
                d.for_each(f); b.for_each(f);
            }
            ExprKind::LetE(_, t, v, b, _) => {
                t.for_each(f); v.for_each(f); b.for_each(f);
            }
            ExprKind::MData(_, e) | ExprKind::Proj(_, _, e) => e.for_each(f),
        }
    }

    /// Check if specific loose bvar index appears.
    pub fn has_loose_bvar(&self, idx: u32) -> bool {
        if self.loose_bvar_range() <= idx {
            return false;
        }
        let mut found = false;
        self.has_loose_bvar_aux(idx as u64, 0, &mut found);
        found
    }

    fn has_loose_bvar_aux(&self, idx: u64, offset: u64, found: &mut bool) {
        if *found || !self.has_loose_bvars() {
            return;
        }
        match &self.inner.kind {
            ExprKind::BVar(i) => {
                if *i == idx + offset {
                    *found = true;
                }
            }
            ExprKind::App(f, a) => {
                f.has_loose_bvar_aux(idx, offset, found);
                a.has_loose_bvar_aux(idx, offset, found);
            }
            ExprKind::Lam(_, d, b, _) | ExprKind::ForallE(_, d, b, _) => {
                d.has_loose_bvar_aux(idx, offset, found);
                b.has_loose_bvar_aux(idx, offset + 1, found);
            }
            ExprKind::LetE(_, t, v, b, _) => {
                t.has_loose_bvar_aux(idx, offset, found);
                v.has_loose_bvar_aux(idx, offset, found);
                b.has_loose_bvar_aux(idx, offset + 1, found);
            }
            ExprKind::MData(_, e) | ExprKind::Proj(_, _, e) => {
                e.has_loose_bvar_aux(idx, offset, found);
            }
            _ => {}
        }
    }

    /// Head beta reduction: `(fun x => body) arg` → `body[0 := arg]`
    pub fn head_beta_reduce(&self) -> Expr {
        if let ExprKind::App(f, a) = &self.inner.kind {
            if f.is_lam() {
                return f.binding_body().instantiate1(a);
            }
        }
        self.clone()
    }

    /// Check if this is a head beta redex.
    pub fn is_head_beta(&self) -> bool {
        matches!(&self.inner.kind, ExprKind::App(f, _) if f.is_lam())
    }
}

// --- Trait implementations ---

impl PartialEq for Expr {
    fn eq(&self, other: &Self) -> bool {
        if Arc::ptr_eq(&self.inner, &other.inner) {
            return true;
        }
        if self.hash() != other.hash() {
            return false;
        }
        self.structural_eq(other)
    }
}

impl Eq for Expr {}

impl Expr {
    fn structural_eq(&self, other: &Expr) -> bool {
        match (&self.inner.kind, &other.inner.kind) {
            (ExprKind::BVar(a), ExprKind::BVar(b)) => a == b,
            (ExprKind::FVar(a), ExprKind::FVar(b)) => a == b,
            (ExprKind::MVar(a), ExprKind::MVar(b)) => a == b,
            (ExprKind::Sort(a), ExprKind::Sort(b)) => a == b,
            (ExprKind::Const(n1, l1), ExprKind::Const(n2, l2)) => n1 == n2 && l1 == l2,
            (ExprKind::App(f1, a1), ExprKind::App(f2, a2)) => f1 == f2 && a1 == a2,
            (ExprKind::Lam(_, d1, b1, _), ExprKind::Lam(_, d2, b2, _)) => d1 == d2 && b1 == b2,
            (ExprKind::ForallE(_, d1, b1, _), ExprKind::ForallE(_, d2, b2, _)) => {
                d1 == d2 && b1 == b2
            }
            (ExprKind::LetE(_, t1, v1, b1, _), ExprKind::LetE(_, t2, v2, b2, _)) => {
                t1 == t2 && v1 == v2 && b1 == b2
            }
            (ExprKind::Lit(a), ExprKind::Lit(b)) => a == b,
            (ExprKind::MData(_, e1), ExprKind::MData(_, e2)) => e1 == e2,
            (ExprKind::Proj(s1, i1, e1), ExprKind::Proj(s2, i2, e2)) => {
                s1 == s2 && i1 == i2 && e1 == e2
            }
            _ => false,
        }
    }
}

impl std::hash::Hash for Expr {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        state.write_u32(self.inner.data.hash);
    }
}

impl fmt::Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.inner.kind {
            ExprKind::BVar(i) => write!(f, "#{}", i),
            ExprKind::FVar(n) => write!(f, "{}", n),
            ExprKind::MVar(n) => write!(f, "?{}", n),
            ExprKind::Sort(l) => {
                if l.is_zero() {
                    write!(f, "Prop")
                } else if l.is_one() {
                    write!(f, "Type")
                } else {
                    write!(f, "Sort {}", l)
                }
            }
            ExprKind::Const(n, ls) => {
                if ls.is_empty() {
                    write!(f, "{}", n)
                } else {
                    write!(f, "{}.{{", n)?;
                    for (i, l) in ls.iter().enumerate() {
                        if i > 0 { write!(f, ", ")?; }
                        write!(f, "{}", l)?;
                    }
                    write!(f, "}}")
                }
            }
            ExprKind::App(fn_, arg) => write!(f, "({} {})", fn_, arg),
            ExprKind::Lam(n, d, b, _) => write!(f, "(fun {} : {} => {})", n, d, b),
            ExprKind::ForallE(n, d, b, _) => {
                if n.is_anonymous() && !b.has_loose_bvar(0) {
                    write!(f, "({} → {})", d, b)
                } else {
                    write!(f, "(∀ {} : {}, {})", n, d, b)
                }
            }
            ExprKind::LetE(n, t, v, b, _) => write!(f, "(let {} : {} := {} in {})", n, t, v, b),
            ExprKind::Lit(l) => write!(f, "{}", l),
            ExprKind::MData(_, e) => write!(f, "{}", e),
            ExprKind::Proj(s, i, e) => write!(f, "{}.{} {}", s, i, e),
        }
    }
}

impl fmt::Debug for Expr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Expr({})", self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bvar() {
        let b = Expr::bvar(0);
        assert!(b.is_bvar());
        assert_eq!(b.bvar_idx(), 0);
        assert!(b.has_loose_bvars());
        assert_eq!(b.loose_bvar_range(), 1);
    }

    #[test]
    fn test_sort() {
        let p = Expr::prop();
        assert!(p.is_sort());
        assert!(p.is_prop());
        assert_eq!(p.to_string(), "Prop");

        let t = Expr::type_();
        assert!(!t.is_prop());
        assert_eq!(t.to_string(), "Type");
    }

    #[test]
    fn test_const() {
        let c = Expr::const_(Name::from_str_parts("Nat.add"), vec![]);
        assert!(c.is_const());
        assert_eq!(c.const_name(), &Name::from_str_parts("Nat.add"));
    }

    #[test]
    fn test_app() {
        let f = Expr::const_(Name::mk_simple("f"), vec![]);
        let a = Expr::const_(Name::mk_simple("a"), vec![]);
        let app = Expr::app(f.clone(), a.clone());
        assert!(app.is_app());
        assert_eq!(app.app_fn(), &f);
        assert_eq!(app.app_arg(), &a);
        assert_eq!(app.get_app_num_args(), 1);
    }

    #[test]
    fn test_lambda() {
        let body = Expr::bvar(0);
        let ty = Expr::type_();
        let lam = Expr::lam(Name::mk_simple("x"), ty.clone(), body, BinderInfo::Default);
        assert!(lam.is_lam());
        assert!(!lam.has_loose_bvars());
        assert_eq!(lam.binding_info(), BinderInfo::Default);
    }

    #[test]
    fn test_forall() {
        let body = Expr::bvar(0);
        let ty = Expr::type_();
        let pi = Expr::forall_e(Name::mk_simple("x"), ty, body, BinderInfo::Default);
        assert!(pi.is_forall());
        assert!(pi.is_binding());
    }

    #[test]
    fn test_instantiate() {
        // (fun x => x) applied gives the substitution
        let body = Expr::bvar(0);
        let c = Expr::const_(Name::mk_simple("c"), vec![]);
        let result = body.instantiate1(&c);
        assert_eq!(result, c);
    }

    #[test]
    fn test_instantiate_nested() {
        // Body uses BVar(0) and BVar(1)
        let body = Expr::app(Expr::bvar(1), Expr::bvar(0));
        let a = Expr::const_(Name::mk_simple("a"), vec![]);
        let b = Expr::const_(Name::mk_simple("b"), vec![]);
        let result = body.instantiate(&[b.clone(), a.clone()]);
        // BVar(0) → subst[0] = b, BVar(1) → subst[1] = a
        assert_eq!(result, Expr::app(a, b));
    }

    #[test]
    fn test_head_beta_reduce() {
        let body = Expr::bvar(0); // identity
        let lam = Expr::lam(Name::mk_simple("x"), Expr::type_(), body, BinderInfo::Default);
        let c = Expr::const_(Name::mk_simple("c"), vec![]);
        let app = Expr::app(lam, c.clone());
        assert!(app.is_head_beta());
        let result = app.head_beta_reduce();
        assert_eq!(result, c);
    }

    #[test]
    fn test_abstract() {
        let c = Expr::const_(Name::mk_simple("c"), vec![]);
        let body = Expr::app(c.clone(), c.clone());
        let abstracted = body.abstract_(&[c.clone()]);
        let expected = Expr::app(Expr::bvar(0), Expr::bvar(0));
        assert_eq!(abstracted, expected);
    }

    #[test]
    fn test_lift_lower() {
        let b0 = Expr::bvar(0);
        let lifted = b0.lift_loose_bvars(0, 2);
        assert_eq!(lifted.bvar_idx(), 2);

        let lowered = lifted.lower_loose_bvars(0, 2);
        assert_eq!(lowered, b0);
    }

    #[test]
    fn test_equality() {
        let a = Expr::const_(Name::mk_simple("Nat"), vec![]);
        let b = Expr::const_(Name::mk_simple("Nat"), vec![]);
        let c = Expr::const_(Name::mk_simple("Bool"), vec![]);
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn test_get_app_args() {
        let f = Expr::const_(Name::mk_simple("f"), vec![]);
        let a1 = Expr::const_(Name::mk_simple("a"), vec![]);
        let a2 = Expr::const_(Name::mk_simple("b"), vec![]);
        let app = Expr::app(Expr::app(f.clone(), a1.clone()), a2.clone());
        let mut args = Vec::new();
        let head = app.get_app_args(&mut args);
        assert_eq!(head, &f);
        assert_eq!(args, vec![a1, a2]);
    }

    #[test]
    fn test_lit() {
        let n = Expr::lit(Literal::nat_small(42));
        assert!(n.is_lit());
        assert!(!n.has_loose_bvars());
        assert_eq!(n.to_string(), "42");
    }

    #[test]
    fn test_proj() {
        let e = Expr::const_(Name::mk_simple("p"), vec![]);
        let p = Expr::proj(Name::mk_simple("Prod"), 0, e);
        assert!(p.is_proj());
        assert_eq!(p.proj_idx(), 0);
    }

    #[test]
    fn test_instantiate_level_params() {
        let u = Name::mk_simple("u");
        let e = Expr::sort(Level::param(u.clone()));
        let result = e.instantiate_level_params(&[u], &[Level::one()]);
        assert_eq!(result, Expr::sort(Level::one()));
    }

    #[test]
    fn test_has_loose_bvar() {
        let body = Expr::app(Expr::bvar(0), Expr::bvar(1));
        assert!(body.has_loose_bvar(0));
        assert!(body.has_loose_bvar(1));
        assert!(!body.has_loose_bvar(2));
    }
}
