use crate::environment::Environment;
use crate::error::{KernelError, KernelResult};
use crate::quot;
use rslean_expr::{
    BinderInfo, ConstantInfo, Declaration, DefinitionSafety, Expr, ExprKind, Literal,
    ReducibilityHints,
};
use rslean_level::Level;
use rslean_name::Name;
use rustc_hash::FxHashMap;

/// The Lean 4 type checker.
///
/// Implements type inference, definitional equality checking, and weak head normal form reduction.
pub struct TypeChecker {
    env: Environment,
    /// Cache: expr → inferred type
    infer_cache: FxHashMap<u64, Expr>,
    /// Cache: expr → whnf
    whnf_cache: FxHashMap<u64, Expr>,
    /// Cache: (expr, expr) → is_def_eq result
    def_eq_cache: FxHashMap<(u64, u64), bool>,
    /// Local context: FVar name → type (for variables introduced during checking)
    local_ctx: FxHashMap<Name, Expr>,
    /// Name generator counter for fresh names
    next_id: u64,
}

impl TypeChecker {
    pub fn new(env: Environment) -> Self {
        TypeChecker {
            env,
            infer_cache: FxHashMap::default(),
            whnf_cache: FxHashMap::default(),
            def_eq_cache: FxHashMap::default(),
            local_ctx: FxHashMap::default(),
            next_id: 0,
        }
    }

    pub fn env(&self) -> &Environment {
        &self.env
    }

    fn fresh_name(&mut self) -> Name {
        let id = self.next_id;
        self.next_id += 1;
        Name::mk_num(Name::mk_simple("_tc"), id)
    }

    // ========================================================================
    // Type inference
    // ========================================================================

    /// Infer the type of an expression (without full checking).
    pub fn infer_type(&mut self, e: &Expr) -> KernelResult<Expr> {
        let key = e.hash() as u64;
        if let Some(cached) = self.infer_cache.get(&key) {
            return Ok(cached.clone());
        }
        let ty = self.infer_type_core(e)?;
        self.infer_cache.insert(key, ty.clone());
        Ok(ty)
    }

    fn infer_type_core(&mut self, e: &Expr) -> KernelResult<Expr> {
        match e.kind() {
            ExprKind::BVar(_) => {
                Err(KernelError::Panic("loose bound variable in infer_type".into()))
            }
            ExprKind::FVar(n) => {
                self.local_ctx
                    .get(n)
                    .cloned()
                    .ok_or_else(|| KernelError::Panic(format!("free variable '{}' not in local context", n)))
            }
            ExprKind::MVar(n) => {
                Err(KernelError::Panic(format!("metavariable '{}' in infer_type", n)))
            }
            ExprKind::Sort(l) => Ok(Expr::sort(Level::succ(l.clone()))),
            ExprKind::Const(name, levels) => self.infer_constant(name, levels),
            ExprKind::App(f, a) => self.infer_app(f, a),
            ExprKind::Lam(name, domain, body, bi) => {
                self.infer_lambda(name, domain, body, *bi)
            }
            ExprKind::ForallE(name, domain, body, bi) => {
                self.infer_pi(name, domain, body, *bi)
            }
            ExprKind::LetE(name, ty, val, body, _non_dep) => {
                self.infer_let(name, ty, val, body)
            }
            ExprKind::Lit(lit) => self.infer_lit(lit),
            ExprKind::MData(_, e) => self.infer_type(e),
            ExprKind::Proj(sname, idx, e) => self.infer_proj(sname, *idx, e),
        }
    }

    fn infer_constant(&mut self, name: &Name, levels: &[Level]) -> KernelResult<Expr> {
        let info = self.env.get(name)?;
        let expected = info.level_params().len();
        let got = levels.len();
        if expected != got {
            return Err(KernelError::IncorrectNumLevels {
                name: name.clone(),
                expected,
                got,
            });
        }
        let ty = info.type_().clone();
        if levels.is_empty() {
            Ok(ty)
        } else {
            let params: Vec<Name> = info.level_params().to_vec();
            Ok(ty.instantiate_level_params(&params, levels))
        }
    }

    fn infer_app(&mut self, f: &Expr, a: &Expr) -> KernelResult<Expr> {
        let f_type = self.infer_type(f)?;
        let f_type = self.whnf(&f_type)?;
        if !f_type.is_forall() {
            return Err(KernelError::FunctionExpected(f.clone()));
        }
        // The type of (f a) is body[0 := a] where f : ∀ (x : T), body
        Ok(f_type.binding_body().instantiate1(a))
    }

    fn infer_lambda(
        &mut self,
        name: &Name,
        domain: &Expr,
        body: &Expr,
        bi: BinderInfo,
    ) -> KernelResult<Expr> {
        // Check domain is a type
        let domain_type = self.infer_type(domain)?;
        self.ensure_sort(&domain_type)?;
        // Infer body type with a free variable for the parameter
        let fvar_name = self.fresh_name();
        self.local_ctx.insert(fvar_name.clone(), domain.clone());
        let fvar = Expr::fvar(fvar_name.clone());
        let body_inst = body.instantiate1(&fvar);
        let body_type = self.infer_type(&body_inst)?;
        self.local_ctx.remove(&fvar_name);
        // Abstract the free variable back to get forall type
        let body_type_abs = body_type.abstract_(&[fvar]);
        Ok(Expr::forall_e(name.clone(), domain.clone(), body_type_abs, bi))
    }

    fn infer_pi(
        &mut self,
        _name: &Name,
        domain: &Expr,
        body: &Expr,
        _bi: BinderInfo,
    ) -> KernelResult<Expr> {
        let domain_type = self.infer_type(domain)?;
        let s1 = self.ensure_sort(&domain_type)?;
        let fvar_name = self.fresh_name();
        self.local_ctx.insert(fvar_name.clone(), domain.clone());
        let fvar = Expr::fvar(fvar_name.clone());
        let body_inst = body.instantiate1(&fvar);
        let body_type = self.infer_type(&body_inst)?;
        self.local_ctx.remove(&fvar_name);
        let s2 = self.ensure_sort(&body_type)?;
        Ok(Expr::sort(Level::imax(s1, s2)))
    }

    fn infer_let(
        &mut self,
        _name: &Name,
        ty: &Expr,
        val: &Expr,
        body: &Expr,
    ) -> KernelResult<Expr> {
        // Check type is a sort
        let ty_type = self.infer_type(ty)?;
        self.ensure_sort(&ty_type)?;
        // Infer body type with let value substituted
        let body_inst = body.instantiate1(val);
        self.infer_type(&body_inst)
    }

    fn infer_lit(&self, lit: &Literal) -> KernelResult<Expr> {
        match lit {
            Literal::Nat(_) => Ok(Expr::const_(Name::mk_simple("Nat"), vec![])),
            Literal::Str(_) => Ok(Expr::const_(Name::mk_simple("String"), vec![])),
        }
    }

    fn infer_proj(&mut self, sname: &Name, idx: u64, e: &Expr) -> KernelResult<Expr> {
        let e_type = self.infer_type(e)?;
        let e_type = self.whnf(&e_type)?;

        let e_type_fn = e_type.get_app_fn();
        if !e_type_fn.is_const() || e_type_fn.const_name() != sname {
            return Err(KernelError::TypeMismatch {
                expected: Expr::const_(sname.clone(), vec![]),
                got: e_type.clone(),
            });
        }

        // Get the inductive type info
        let info = self.env.get(sname)?;
        if let ConstantInfo::Inductive { ctors, num_params, .. } = info {
            if ctors.len() != 1 {
                return Err(KernelError::InductiveError(
                    format!("projection on non-structure type '{}'", sname),
                ));
            }
            let ctor_name = &ctors[0];
            let ctor_info = self.env.get(ctor_name)?;
            let ctor_type = ctor_info.type_().clone();
            let levels = e_type_fn.const_levels();
            let params: Vec<Name> = info.level_params().to_vec();
            let ctor_type = ctor_type.instantiate_level_params(&params, levels);

            // Skip params
            let mut ty = ctor_type;
            let mut e_type_args = Vec::new();
            e_type.get_app_args(&mut e_type_args);

            for i in 0..(*num_params as usize) {
                if !ty.is_forall() {
                    return Err(KernelError::InductiveError("projection type error".into()));
                }
                ty = ty.binding_body().instantiate1(
                    if i < e_type_args.len() { &e_type_args[i] } else { e },
                );
            }

            // Skip to the idx-th field
            for i in 0..idx {
                if !ty.is_forall() {
                    return Err(KernelError::InductiveError("projection index out of range".into()));
                }
                // Each field type may depend on previous fields via projection
                let proj_i = Expr::proj(sname.clone(), i, e.clone());
                ty = ty.binding_body().instantiate1(&proj_i);
            }

            if ty.is_forall() {
                Ok(ty.binding_domain().clone())
            } else {
                Err(KernelError::InductiveError("projection index out of range".into()))
            }
        } else {
            Err(KernelError::InductiveError(
                format!("'{}' is not an inductive type", sname),
            ))
        }
    }

    // ========================================================================
    // Weak Head Normal Form (WHNF)
    // ========================================================================

    /// Reduce an expression to weak head normal form.
    pub fn whnf(&mut self, e: &Expr) -> KernelResult<Expr> {
        let key = e.hash() as u64;
        if let Some(cached) = self.whnf_cache.get(&key) {
            return Ok(cached.clone());
        }
        let result = self.whnf_core(e)?;
        self.whnf_cache.insert(key, result.clone());
        Ok(result)
    }

    fn whnf_core(&mut self, e: &Expr) -> KernelResult<Expr> {
        let mut e = e.clone();
        loop {
            match e.kind() {
                // Already in WHNF
                ExprKind::BVar(_) | ExprKind::FVar(_) | ExprKind::MVar(_) | ExprKind::Sort(_)
                | ExprKind::Lam(..) | ExprKind::ForallE(..) | ExprKind::Lit(_) => return Ok(e),

                // Delta reduction: unfold definitions
                ExprKind::Const(_name, _levels) => {
                    if let Some(unfolded) = self.unfold_definition(&e)? {
                        e = unfolded;
                        continue;
                    }
                    return Ok(e);
                }

                // Beta reduction
                ExprKind::App(_, _) => {
                    if e.is_head_beta() {
                        e = e.head_beta_reduce();
                        continue;
                    }
                    // Try to reduce the head
                    let fn_ = e.get_app_fn();
                    match fn_.kind() {
                        ExprKind::Const(_, _) => {
                            // Try delta + recursor reduction
                            if let Some(reduced) = self.try_reduce_app(&e)? {
                                e = reduced;
                                continue;
                            }
                            return Ok(e);
                        }
                        ExprKind::Lam(..) => {
                            // Collect all args and beta-reduce
                            let mut args = Vec::new();
                            let head = e.get_app_args(&mut args);
                            let mut result = head.clone();
                            for arg in &args {
                                if result.is_lam() {
                                    result = result.binding_body().instantiate1(arg);
                                } else {
                                    result = Expr::app(result, arg.clone());
                                }
                            }
                            e = result;
                            continue;
                        }
                        _ => return Ok(e),
                    }
                }

                // Zeta reduction: let
                ExprKind::LetE(_, _, val, body, _) => {
                    e = body.instantiate1(val);
                    continue;
                }

                // MData: strip metadata
                ExprKind::MData(_, inner) => {
                    e = inner.clone();
                    continue;
                }

                // Projection reduction
                ExprKind::Proj(sname, idx, struct_expr) => {
                    if let Some(reduced) = self.reduce_proj(sname, *idx, struct_expr)? {
                        e = reduced;
                        continue;
                    }
                    return Ok(e);
                }
            }
        }
    }

    /// Try to reduce an application at the head.
    fn try_reduce_app(&mut self, e: &Expr) -> KernelResult<Option<Expr>> {
        let fn_ = e.get_app_fn();
        let fn_name = fn_.const_name();

        // Try recursor reduction
        if self.env.is_recursor(fn_name) {
            if let Some(reduced) = self.reduce_recursor(e)? {
                return Ok(Some(reduced));
            }
        }

        // Try quotient reduction
        if self.env.is_quot_initialized() && quot::is_quot_rec(fn_name) {
            let env = self.env.clone();
            if let Some(reduced) = quot::quot_reduce_rec(e, &|x| {
                // We need a simple whnf for the quotient reduction
                // This is a simplified version that just does basic reduction
                let mut tc = TypeChecker::new(env.clone());
                tc.whnf(x).unwrap_or_else(|_| x.clone())
            }) {
                return Ok(Some(reduced));
            }
        }

        // Try delta reduction (unfold the head)
        if let Some(unfolded) = self.unfold_definition(fn_)? {
            let mut args = Vec::new();
            e.get_app_args(&mut args);
            return Ok(Some(Expr::mk_app(unfolded, &args)));
        }

        Ok(None)
    }

    /// Unfold a definition constant.
    fn unfold_definition(&self, e: &Expr) -> KernelResult<Option<Expr>> {
        if !e.is_const() {
            return Ok(None);
        }
        let name = e.const_name();
        let levels = e.const_levels();
        match self.env.find(name) {
            Some(ConstantInfo::Definition { value, level_params, safety, .. }) => {
                if *safety == DefinitionSafety::Unsafe {
                    return Ok(None); // Don't unfold unsafe
                }
                let v = if levels.is_empty() {
                    value.clone()
                } else {
                    value.instantiate_level_params(level_params, levels)
                };
                Ok(Some(v))
            }
            _ => Ok(None),
        }
    }

    /// Reduce a projection expression.
    fn reduce_proj(
        &mut self,
        sname: &Name,
        idx: u64,
        struct_expr: &Expr,
    ) -> KernelResult<Option<Expr>> {
        let s = self.whnf(struct_expr)?;
        let s_fn = s.get_app_fn();
        if !s_fn.is_const() {
            return Ok(None);
        }
        // Check if the constructor matches the structure
        if let Some(ConstantInfo::Constructor { induct_name, num_params, num_fields, .. }) =
            self.env.find(s_fn.const_name())
        {
            if induct_name != sname {
                return Ok(None);
            }
            let field_idx = *num_params as u64 + idx;
            let mut args = Vec::new();
            s.get_app_args(&mut args);
            if field_idx < args.len() as u64 {
                return Ok(Some(args[field_idx as usize].clone()));
            }
            let _ = num_fields; // will be used for validation later
        }
        Ok(None)
    }

    /// Reduce a recursor application (iota reduction).
    fn reduce_recursor(&mut self, e: &Expr) -> KernelResult<Option<Expr>> {
        let fn_ = e.get_app_fn();
        let fn_name = fn_.const_name();
        let rec_info = match self.env.find(fn_name) {
            Some(ConstantInfo::Recursor {
                num_params,
                num_indices,
                num_motives,
                num_minors,
                rules,
                ..
            }) => (
                *num_params,
                *num_indices,
                *num_motives,
                *num_minors,
                rules.clone(),
            ),
            _ => return Ok(None),
        };
        let (num_params, num_indices, num_motives, num_minors, rules) = rec_info;

        let mut args = Vec::new();
        e.get_app_args(&mut args);

        // Major premise index = num_params + num_motives + num_minors + num_indices
        let major_idx =
            (num_params + num_motives + num_minors + num_indices) as usize;
        if args.len() <= major_idx {
            return Ok(None);
        }

        let major = self.whnf(&args[major_idx])?;
        let major_fn = major.get_app_fn();
        if !major_fn.is_const() {
            return Ok(None);
        }

        // Find matching recursor rule
        let ctor_name = major_fn.const_name();
        let rule = rules.iter().find(|r| r.ctor_name == *ctor_name);
        let rule = match rule {
            Some(r) => r.clone(),
            None => return Ok(None),
        };

        let mut major_args = Vec::new();
        major.get_app_args(&mut major_args);

        // Build the result: rule.rhs applied to params, then to constructor fields
        let mut result = rule.rhs.clone();

        // Apply universe level params
        let levels = fn_.const_levels();
        if !levels.is_empty() {
            if let Some(ConstantInfo::Recursor { level_params, .. }) = self.env.find(fn_name) {
                result = result.instantiate_level_params(level_params, levels);
            }
        }

        // Apply: params, motives, minors
        let pre_args_count = (num_params + num_motives + num_minors) as usize;
        for i in 0..std::cmp::min(pre_args_count, args.len()) {
            result = Expr::app(result, args[i].clone());
        }

        // Apply constructor fields (skip params in major_args)
        let num_params_usize = num_params as usize;
        for i in num_params_usize..std::cmp::min(
            num_params_usize + rule.num_fields as usize,
            major_args.len(),
        ) {
            result = Expr::app(result, major_args[i].clone());
        }

        // Apply remaining args after major
        for arg in &args[major_idx + 1..] {
            result = Expr::app(result, arg.clone());
        }

        Ok(Some(result))
    }

    // ========================================================================
    // Definitional Equality
    // ========================================================================

    /// Check if two expressions are definitionally equal.
    pub fn is_def_eq(&mut self, a: &Expr, b: &Expr) -> KernelResult<bool> {
        // Pointer equality
        if a == b {
            return Ok(true);
        }

        // Cache check
        let key = (a.hash() as u64, b.hash() as u64);
        if let Some(&cached) = self.def_eq_cache.get(&key) {
            return Ok(cached);
        }

        let result = self.is_def_eq_core(a, b)?;
        self.def_eq_cache.insert(key, result);
        Ok(result)
    }

    fn is_def_eq_core(&mut self, a: &Expr, b: &Expr) -> KernelResult<bool> {
        // Quick structural checks
        if a == b {
            return Ok(true);
        }

        // WHNF both sides
        let a = self.whnf(a)?;
        let b = self.whnf(b)?;

        if a == b {
            return Ok(true);
        }

        // Kind-specific checks
        match (a.kind(), b.kind()) {
            (ExprKind::Sort(l1), ExprKind::Sort(l2)) => {
                return Ok(l1.is_equivalent(l2));
            }
            (ExprKind::Const(n1, ls1), ExprKind::Const(n2, ls2)) => {
                if n1 == n2 && ls1.len() == ls2.len() {
                    let all_eq = ls1
                        .iter()
                        .zip(ls2.iter())
                        .all(|(l1, l2)| l1.is_equivalent(l2));
                    if all_eq {
                        return Ok(true);
                    }
                }
            }
            (ExprKind::BVar(i1), ExprKind::BVar(i2)) => return Ok(i1 == i2),
            (ExprKind::FVar(n1), ExprKind::FVar(n2)) => return Ok(n1 == n2),
            _ => {}
        }

        // Lambda/ForallE: compare structurally under binder
        if a.is_lam() && b.is_lam() {
            return self.is_def_eq_binding(&a, &b);
        }
        if a.is_forall() && b.is_forall() {
            return self.is_def_eq_binding(&a, &b);
        }

        // App: compare head + args
        if a.is_app() && b.is_app() {
            if self.is_def_eq_app(&a, &b)? {
                return Ok(true);
            }
        }

        // Proof irrelevance: if both have type Prop, they're equal
        if let Ok(true) = self.is_proof_irrel(&a, &b) {
            return Ok(true);
        }

        // Eta expansion
        if self.try_eta(&a, &b)? {
            return Ok(true);
        }

        Ok(false)
    }

    fn is_def_eq_binding(&mut self, a: &Expr, b: &Expr) -> KernelResult<bool> {
        // Compare domains
        if !self.is_def_eq(a.binding_domain(), b.binding_domain())? {
            return Ok(false);
        }
        // Compare bodies with a fresh variable
        let fvar_name = self.fresh_name();
        self.local_ctx.insert(fvar_name.clone(), a.binding_domain().clone());
        let fvar = Expr::fvar(fvar_name.clone());
        let a_body = a.binding_body().instantiate1(&fvar);
        let b_body = b.binding_body().instantiate1(&fvar);
        let result = self.is_def_eq(&a_body, &b_body);
        self.local_ctx.remove(&fvar_name);
        result
    }

    fn is_def_eq_app(&mut self, a: &Expr, b: &Expr) -> KernelResult<bool> {
        let mut a_args = Vec::new();
        let mut b_args = Vec::new();
        let a_fn = a.get_app_args(&mut a_args);
        let b_fn = b.get_app_args(&mut b_args);
        if a_args.len() != b_args.len() {
            return Ok(false);
        }
        if !self.is_def_eq(a_fn, b_fn)? {
            return Ok(false);
        }
        for (aa, ba) in a_args.iter().zip(b_args.iter()) {
            if !self.is_def_eq(aa, ba)? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn is_proof_irrel(&mut self, a: &Expr, b: &Expr) -> KernelResult<bool> {
        let a_type = self.infer_type(a)?;
        let a_type = self.whnf(&a_type)?;
        if a_type.is_prop() {
            return Ok(true);
        }
        // Check if a_type : Prop
        let a_type_type = self.infer_type(&a_type)?;
        let a_type_type = self.whnf(&a_type_type)?;
        if a_type_type.is_prop() {
            // Both a and b have types that are propositions → proof irrelevant
            let b_type = self.infer_type(b)?;
            if self.is_def_eq(&a_type, &b_type)? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn try_eta(&mut self, a: &Expr, b: &Expr) -> KernelResult<bool> {
        // Eta for lambda: a =?= fun x => a x (when a is not lambda)
        if a.is_lam() && !b.is_lam() {
            let b_eta = Expr::lam(
                a.binding_name().clone(),
                a.binding_domain().clone(),
                Expr::app(b.lift_loose_bvars(0, 1), Expr::bvar(0)),
                a.binding_info(),
            );
            return self.is_def_eq(a, &b_eta);
        }
        if b.is_lam() && !a.is_lam() {
            let a_eta = Expr::lam(
                b.binding_name().clone(),
                b.binding_domain().clone(),
                Expr::app(a.lift_loose_bvars(0, 1), Expr::bvar(0)),
                b.binding_info(),
            );
            return self.is_def_eq(&a_eta, b);
        }
        Ok(false)
    }

    // ========================================================================
    // Checking helpers
    // ========================================================================

    /// Ensure an expression is a Sort, returning the level. If not, reduce to WHNF first.
    pub fn ensure_sort(&mut self, e: &Expr) -> KernelResult<Level> {
        let e = self.whnf(e)?;
        match e.kind() {
            ExprKind::Sort(l) => Ok(l.clone()),
            _ => Err(KernelError::TypeExpected(e)),
        }
    }

    /// Ensure an expression has a Pi type, returning the Pi type.
    pub fn ensure_pi(&mut self, e: &Expr) -> KernelResult<Expr> {
        let e = self.whnf(e)?;
        if e.is_forall() {
            Ok(e)
        } else {
            Err(KernelError::FunctionExpected(e))
        }
    }

    // ========================================================================
    // Declaration checking
    // ========================================================================

    /// Check a declaration and add it to the environment.
    pub fn check_and_add(&mut self, decl: Declaration) -> KernelResult<Environment> {
        match decl {
            Declaration::Axiom { name, level_params, type_, is_unsafe } => {
                self.check_no_mvar_fvar(&name, &type_)?;
                let _sort = self.check_type(&type_, &level_params)?;
                let info = ConstantInfo::Axiom { name, level_params, type_, is_unsafe };
                self.env = self.env.add_constant(info)?;
                Ok(self.env.clone())
            }
            Declaration::Definition { name, level_params, type_, value, hints, safety } => {
                self.check_no_mvar_fvar(&name, &type_)?;
                self.check_no_mvar_fvar(&name, &value)?;
                let _sort = self.check_type(&type_, &level_params)?;
                let val_type = self.infer_type(&value)?;
                if !self.is_def_eq(&type_, &val_type)? {
                    return Err(KernelError::TypeMismatch {
                        expected: type_.clone(),
                        got: val_type,
                    });
                }
                let info = ConstantInfo::Definition {
                    name, level_params, type_, value, hints, safety,
                };
                self.env = self.env.add_constant(info)?;
                Ok(self.env.clone())
            }
            Declaration::Theorem { name, level_params, type_, value } => {
                self.check_no_mvar_fvar(&name, &type_)?;
                self.check_no_mvar_fvar(&name, &value)?;
                let _sort = self.check_type(&type_, &level_params)?;
                let val_type = self.infer_type(&value)?;
                if !self.is_def_eq(&type_, &val_type)? {
                    return Err(KernelError::TypeMismatch {
                        expected: type_.clone(),
                        got: val_type,
                    });
                }
                let info = ConstantInfo::Theorem { name, level_params, type_, value };
                self.env = self.env.add_constant(info)?;
                Ok(self.env.clone())
            }
            Declaration::Opaque { name, level_params, type_, value, is_unsafe } => {
                self.check_no_mvar_fvar(&name, &type_)?;
                self.check_no_mvar_fvar(&name, &value)?;
                let _sort = self.check_type(&type_, &level_params)?;
                let val_type = self.infer_type(&value)?;
                if !self.is_def_eq(&type_, &val_type)? {
                    return Err(KernelError::TypeMismatch {
                        expected: type_.clone(),
                        got: val_type,
                    });
                }
                let info = ConstantInfo::Opaque { name, level_params, type_, value, is_unsafe };
                self.env = self.env.add_constant(info)?;
                Ok(self.env.clone())
            }
            Declaration::Quot => {
                self.env = self.env.set_quot_initialized();
                Ok(self.env.clone())
            }
            Declaration::InductiveDecl { .. } => {
                // Inductive type checking is complex — placeholder for now
                Err(KernelError::InductiveError("inductive checking not yet implemented".into()))
            }
        }
    }

    /// Check that a type expression is valid (infer its type and ensure it's a Sort).
    fn check_type(&mut self, type_: &Expr, _level_params: &[Name]) -> KernelResult<Level> {
        let ty = self.infer_type(type_)?;
        self.ensure_sort(&ty)
    }

    fn check_no_mvar_fvar(&self, name: &Name, e: &Expr) -> KernelResult<()> {
        if e.has_expr_mvar() || e.has_univ_mvar() {
            return Err(KernelError::HasMVar(name.clone()));
        }
        if e.has_fvar() {
            return Err(KernelError::HasFVar(name.clone()));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_nat_env() -> Environment {
        let env = Environment::new();
        // Nat : Type
        let nat_info = ConstantInfo::Inductive {
            name: Name::mk_simple("Nat"),
            level_params: vec![],
            type_: Expr::type_(),
            num_params: 0,
            num_indices: 0,
            all: vec![Name::mk_simple("Nat")],
            ctors: vec![Name::from_str_parts("Nat.zero"), Name::from_str_parts("Nat.succ")],
            num_nested: 0,
            is_rec: true,
            is_unsafe: false,
            is_reflexive: false,
        };
        let env = env.add_constant_unchecked(nat_info);

        // Nat.zero : Nat
        let zero_info = ConstantInfo::Constructor {
            name: Name::from_str_parts("Nat.zero"),
            level_params: vec![],
            type_: Expr::const_(Name::mk_simple("Nat"), vec![]),
            induct_name: Name::mk_simple("Nat"),
            ctor_idx: 0,
            num_params: 0,
            num_fields: 0,
            is_unsafe: false,
        };
        let env = env.add_constant_unchecked(zero_info);

        // Nat.succ : Nat → Nat
        let succ_type = Expr::forall_e(
            Name::mk_simple("n"),
            Expr::const_(Name::mk_simple("Nat"), vec![]),
            Expr::const_(Name::mk_simple("Nat"), vec![]),
            BinderInfo::Default,
        );
        let succ_info = ConstantInfo::Constructor {
            name: Name::from_str_parts("Nat.succ"),
            level_params: vec![],
            type_: succ_type,
            induct_name: Name::mk_simple("Nat"),
            ctor_idx: 1,
            num_params: 0,
            num_fields: 1,
            is_unsafe: false,
        };
        env.add_constant_unchecked(succ_info)
    }

    #[test]
    fn test_infer_sort() {
        let env = Environment::new();
        let mut tc = TypeChecker::new(env);
        let ty = tc.infer_type(&Expr::prop()).unwrap();
        // Prop : Type = Sort 1
        assert_eq!(ty, Expr::sort(Level::one()));
    }

    #[test]
    fn test_infer_const() {
        let env = setup_nat_env();
        let mut tc = TypeChecker::new(env);
        let nat = Expr::const_(Name::mk_simple("Nat"), vec![]);
        let ty = tc.infer_type(&nat).unwrap();
        // Nat : Type
        assert_eq!(ty, Expr::type_());
    }

    #[test]
    fn test_infer_app() {
        let env = setup_nat_env();
        let mut tc = TypeChecker::new(env);
        let succ = Expr::const_(Name::from_str_parts("Nat.succ"), vec![]);
        let zero = Expr::const_(Name::from_str_parts("Nat.zero"), vec![]);
        let app = Expr::app(succ, zero);
        let ty = tc.infer_type(&app).unwrap();
        assert_eq!(ty, Expr::const_(Name::mk_simple("Nat"), vec![]));
    }

    #[test]
    fn test_infer_lambda() {
        let env = setup_nat_env();
        let mut tc = TypeChecker::new(env);
        // fun (x : Nat) => x
        let id = Expr::lam(
            Name::mk_simple("x"),
            Expr::const_(Name::mk_simple("Nat"), vec![]),
            Expr::bvar(0),
            BinderInfo::Default,
        );
        let ty = tc.infer_type(&id).unwrap();
        // Should be Nat → Nat
        assert!(ty.is_forall());
    }

    #[test]
    fn test_whnf_delta() {
        let env = setup_nat_env();
        // Add: myDef := Nat.zero
        let info = ConstantInfo::Definition {
            name: Name::mk_simple("myDef"),
            level_params: vec![],
            type_: Expr::const_(Name::mk_simple("Nat"), vec![]),
            value: Expr::const_(Name::from_str_parts("Nat.zero"), vec![]),
            hints: ReducibilityHints::Regular(1),
            safety: DefinitionSafety::Safe,
        };
        let env = env.add_constant_unchecked(info);
        let mut tc = TypeChecker::new(env);

        let my_def = Expr::const_(Name::mk_simple("myDef"), vec![]);
        let reduced = tc.whnf(&my_def).unwrap();
        assert_eq!(reduced, Expr::const_(Name::from_str_parts("Nat.zero"), vec![]));
    }

    #[test]
    fn test_whnf_beta() {
        let env = setup_nat_env();
        let mut tc = TypeChecker::new(env);
        // (fun x => x) Nat.zero → Nat.zero
        let id = Expr::lam(
            Name::mk_simple("x"),
            Expr::const_(Name::mk_simple("Nat"), vec![]),
            Expr::bvar(0),
            BinderInfo::Default,
        );
        let zero = Expr::const_(Name::from_str_parts("Nat.zero"), vec![]);
        let app = Expr::app(id, zero.clone());
        let reduced = tc.whnf(&app).unwrap();
        assert_eq!(reduced, zero);
    }

    #[test]
    fn test_whnf_let() {
        let env = setup_nat_env();
        let mut tc = TypeChecker::new(env);
        let nat = Expr::const_(Name::mk_simple("Nat"), vec![]);
        let zero = Expr::const_(Name::from_str_parts("Nat.zero"), vec![]);
        let let_expr = Expr::let_e(
            Name::mk_simple("x"),
            nat,
            zero.clone(),
            Expr::bvar(0),
            false,
        );
        let reduced = tc.whnf(&let_expr).unwrap();
        assert_eq!(reduced, zero);
    }

    #[test]
    fn test_def_eq_simple() {
        let env = setup_nat_env();
        let mut tc = TypeChecker::new(env);
        let nat = Expr::const_(Name::mk_simple("Nat"), vec![]);
        assert!(tc.is_def_eq(&nat, &nat).unwrap());

        let zero = Expr::const_(Name::from_str_parts("Nat.zero"), vec![]);
        assert!(!tc.is_def_eq(&nat, &zero).unwrap());
    }

    #[test]
    fn test_def_eq_delta() {
        let env = setup_nat_env();
        let zero = Expr::const_(Name::from_str_parts("Nat.zero"), vec![]);
        let info = ConstantInfo::Definition {
            name: Name::mk_simple("myZero"),
            level_params: vec![],
            type_: Expr::const_(Name::mk_simple("Nat"), vec![]),
            value: zero.clone(),
            hints: ReducibilityHints::Regular(1),
            safety: DefinitionSafety::Safe,
        };
        let env = env.add_constant_unchecked(info);
        let mut tc = TypeChecker::new(env);

        let my_zero = Expr::const_(Name::mk_simple("myZero"), vec![]);
        assert!(tc.is_def_eq(&my_zero, &zero).unwrap());
    }

    #[test]
    fn test_def_eq_universe_levels() {
        let env = Environment::new();
        let mut tc = TypeChecker::new(env);
        let s1 = Expr::sort(Level::zero());
        let s2 = Expr::sort(Level::zero());
        assert!(tc.is_def_eq(&s1, &s2).unwrap());

        let s3 = Expr::sort(Level::one());
        assert!(!tc.is_def_eq(&s1, &s3).unwrap());
    }

    #[test]
    fn test_check_axiom() {
        let env = Environment::new();
        let mut tc = TypeChecker::new(env);
        let decl = Declaration::Axiom {
            name: Name::mk_simple("MyType"),
            level_params: vec![],
            type_: Expr::type_(),
            is_unsafe: false,
        };
        let new_env = tc.check_and_add(decl).unwrap();
        assert!(new_env.find(&Name::mk_simple("MyType")).is_some());
    }

    #[test]
    fn test_check_definition() {
        let env = setup_nat_env();
        let mut tc = TypeChecker::new(env);
        let nat = Expr::const_(Name::mk_simple("Nat"), vec![]);
        let zero = Expr::const_(Name::from_str_parts("Nat.zero"), vec![]);
        let decl = Declaration::Definition {
            name: Name::mk_simple("myVal"),
            level_params: vec![],
            type_: nat,
            value: zero,
            hints: ReducibilityHints::Regular(1),
            safety: DefinitionSafety::Safe,
        };
        let new_env = tc.check_and_add(decl).unwrap();
        assert!(new_env.find(&Name::mk_simple("myVal")).is_some());
    }

    #[test]
    fn test_infer_lit() {
        let env = setup_nat_env();
        // Add String type
        let str_info = ConstantInfo::Axiom {
            name: Name::mk_simple("String"),
            level_params: vec![],
            type_: Expr::type_(),
            is_unsafe: false,
        };
        let env = env.add_constant_unchecked(str_info);
        let mut tc = TypeChecker::new(env);

        let nat_lit = Expr::lit(Literal::nat_small(42));
        let ty = tc.infer_type(&nat_lit).unwrap();
        assert_eq!(ty, Expr::const_(Name::mk_simple("Nat"), vec![]));

        let str_lit = Expr::lit(Literal::string("hello"));
        let ty = tc.infer_type(&str_lit).unwrap();
        assert_eq!(ty, Expr::const_(Name::mk_simple("String"), vec![]));
    }
}
