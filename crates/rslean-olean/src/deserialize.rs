use crate::error::{OleanError, OleanResult};
use crate::region::{CompactedRegion, ObjRef, LEAN_TAG_ARRAY, LEAN_TAG_MPZ, LEAN_TAG_STRING};
use rslean_expr::{
    BinderInfo, ConstantInfo, DefinitionSafety, Expr, Literal, MData, MDataValue, QuotKind,
    RecursorRule, ReducibilityHints,
};
use rslean_level::Level;
use rslean_name::Name;
use rustc_hash::FxHashMap;

/// Module data extracted from an .olean file.
#[derive(Debug)]
pub struct ModuleData {
    pub imports: Vec<Import>,
    pub const_names: Vec<Name>,
    pub constants: Vec<ConstantInfo>,
    pub extra_const_names: Vec<Name>,
}

/// An import declaration in a module.
#[derive(Debug, Clone)]
pub struct Import {
    pub module: Name,
    pub import_all: bool,
    pub is_exported: bool,
    pub is_meta: bool,
}

/// Type-directed deserializer for Lean objects in a compacted region.
pub struct Deserializer<'a> {
    region: &'a CompactedRegion,
    name_cache: FxHashMap<usize, Name>,
    level_cache: FxHashMap<usize, Level>,
    expr_cache: FxHashMap<usize, Expr>,
}

impl<'a> Deserializer<'a> {
    pub fn new(region: &'a CompactedRegion) -> Self {
        Deserializer {
            region,
            name_cache: FxHashMap::default(),
            level_cache: FxHashMap::default(),
            expr_cache: FxHashMap::default(),
        }
    }

    /// Deserialize the root object as ModuleData.
    pub fn read_module_data(&mut self) -> OleanResult<ModuleData> {
        let root = self.region.root();
        self.deser_module_data(root)
    }

    // ─── ModuleData ─────────────────────────────────────────────────────
    // Structure with 5 object fields + 1 scalar (Bool):
    //   obj 0: imports (Array Import)
    //   obj 1: constNames (Array Name)
    //   obj 2: constants (Array ConstantInfo)
    //   obj 3: extraConstNames (Array Name)
    //   obj 4: entries (Array (Name × Array EnvExtensionEntry)) — skipped
    //   scalar 0: isModule (Bool, u8)

    fn deser_module_data(&mut self, obj: ObjRef) -> OleanResult<ModuleData> {
        let pos = self.expect_ptr(obj, "ModuleData")?;
        let tag = self.region.obj_tag(pos);
        if tag != 0 {
            return Err(OleanError::Deserialize(format!(
                "ModuleData: expected tag 0, got {}",
                tag
            )));
        }

        let imports_ref = self.region.ctor_obj_field(pos, 0);
        let const_names_ref = self.region.ctor_obj_field(pos, 1);
        let constants_ref = self.region.ctor_obj_field(pos, 2);
        let extra_const_names_ref = self.region.ctor_obj_field(pos, 3);
        // obj 4 = entries — we skip these

        let imports = self.deser_array(imports_ref, |d, r| d.deser_import(r))?;
        let const_names = self.deser_array(const_names_ref, |d, r| Ok(d.deser_name(r)))?;
        let constants = self.deser_array(constants_ref, |d, r| d.deser_constant_info(r))?;
        let extra_const_names =
            self.deser_array(extra_const_names_ref, |d, r| Ok(d.deser_name(r)))?;

        Ok(ModuleData {
            imports,
            const_names,
            constants,
            extra_const_names,
        })
    }

    // ─── Import ─────────────────────────────────────────────────────────
    // Structure (tag 0): obj[module: Name], scalar[importAll: u8, isExported: u8, isMeta: u8]

    fn deser_import(&mut self, obj: ObjRef) -> OleanResult<Import> {
        let pos = self.expect_ptr(obj, "Import")?;
        let module_ref = self.region.ctor_obj_field(pos, 0);
        let module = self.deser_name(module_ref);
        let import_all = self.region.ctor_scalar_u8(pos, 1, 0) != 0;
        let is_exported = self.region.ctor_scalar_u8(pos, 1, 1) != 0;
        let is_meta = self.region.ctor_scalar_u8(pos, 1, 2) != 0;
        Ok(Import {
            module,
            import_all,
            is_exported,
            is_meta,
        })
    }

    // ─── Name ───────────────────────────────────────────────────────────
    // Tag 0: anonymous (no fields) — stored as Scalar(0)
    // Tag 1: str (prefix: Name, str: String) — 2 obj fields
    // Tag 2: num (prefix: Name, i: Nat) — 2 obj fields

    pub fn deser_name(&mut self, obj: ObjRef) -> Name {
        match obj {
            ObjRef::Null => Name::anonymous(),
            ObjRef::Scalar(0) => Name::anonymous(),
            ObjRef::Scalar(_) => Name::anonymous(), // shouldn't happen, but be safe
            ObjRef::Ptr(pos) => {
                if let Some(cached) = self.name_cache.get(&pos) {
                    return cached.clone();
                }
                let tag = self.region.obj_tag(pos);
                let name = match tag {
                    0 => Name::anonymous(),
                    1 => {
                        // str(prefix, string)
                        let prefix_ref = self.region.ctor_obj_field(pos, 0);
                        let string_ref = self.region.ctor_obj_field(pos, 1);
                        let prefix = self.deser_name(prefix_ref);
                        let s = self.deser_string_obj(string_ref);
                        Name::mk_str(prefix, &s)
                    }
                    2 => {
                        // num(prefix, nat)
                        let prefix_ref = self.region.ctor_obj_field(pos, 0);
                        let nat_ref = self.region.ctor_obj_field(pos, 1);
                        let prefix = self.deser_name(prefix_ref);
                        let n = self.deser_nat(nat_ref);
                        Name::mk_num(prefix, n)
                    }
                    _ => Name::anonymous(),
                };
                self.name_cache.insert(pos, name.clone());
                name
            }
        }
    }

    // ─── Level ──────────────────────────────────────────────────────────
    // Tag 0: zero — Scalar(0)
    // Tag 1: succ(level) — 1 obj field
    // Tag 2: max(l1, l2) — 2 obj fields
    // Tag 3: imax(l1, l2) — 2 obj fields
    // Tag 4: param(name) — 1 obj field
    // Tag 5: mvar(name) — 1 obj field (LMVarId is unboxed to Name)

    fn deser_level(&mut self, obj: ObjRef) -> Level {
        match obj {
            ObjRef::Null | ObjRef::Scalar(0) => Level::zero(),
            ObjRef::Scalar(_tag) => {
                // Other scalar levels shouldn't normally appear
                Level::zero()
            }
            ObjRef::Ptr(pos) => {
                if let Some(cached) = self.level_cache.get(&pos) {
                    return cached.clone();
                }
                let tag = self.region.obj_tag(pos);
                let level = match tag {
                    0 => Level::zero(),
                    1 => {
                        let inner = self.region.ctor_obj_field(pos, 0);
                        Level::succ(self.deser_level(inner))
                    }
                    2 => {
                        let l1 = self.region.ctor_obj_field(pos, 0);
                        let l2 = self.region.ctor_obj_field(pos, 1);
                        Level::max(self.deser_level(l1), self.deser_level(l2))
                    }
                    3 => {
                        let l1 = self.region.ctor_obj_field(pos, 0);
                        let l2 = self.region.ctor_obj_field(pos, 1);
                        Level::imax(self.deser_level(l1), self.deser_level(l2))
                    }
                    4 => {
                        let name_ref = self.region.ctor_obj_field(pos, 0);
                        Level::param(self.deser_name(name_ref))
                    }
                    5 => {
                        // LMVarId is an unboxed wrapper around Name
                        let name_ref = self.region.ctor_obj_field(pos, 0);
                        Level::mvar(self.deser_name(name_ref))
                    }
                    _ => Level::zero(),
                };
                self.level_cache.insert(pos, level.clone());
                level
            }
        }
    }

    // ─── Expr ───────────────────────────────────────────────────────────
    // Tag 0:  bvar(nat)                  — 1 obj
    // Tag 1:  fvar(FVarId=Name)          — 1 obj
    // Tag 2:  mvar(MVarId=Name)          — 1 obj
    // Tag 3:  sort(level)                — 1 obj
    // Tag 4:  const(name, List Level)    — 2 obj
    // Tag 5:  app(fn, arg)               — 2 obj
    // Tag 6:  lam(name, type, body, bi)  — 3 obj + 1 scalar u8
    // Tag 7:  forallE(name, ty, body, bi)— 3 obj + 1 scalar u8
    // Tag 8:  letE(name, ty, val, body, nondep) — 4 obj + 1 scalar u8
    // Tag 9:  lit(Literal)               — 1 obj
    // Tag 10: mdata(MData, expr)         — 2 obj
    // Tag 11: proj(name, nat, struct)    — 2 obj + 1 scalar (nat as u64 in scalar area)
    //         Actually: proj has 3 obj fields: name, nat (as obj), struct — let me check

    fn deser_expr(&mut self, obj: ObjRef) -> OleanResult<Expr> {
        match obj {
            ObjRef::Null => Err(OleanError::Deserialize("null Expr".into())),
            ObjRef::Scalar(n) => {
                // Expr constructors with no fields don't exist, so this shouldn't happen.
                // But just in case, treat as bvar.
                Ok(Expr::bvar(n))
            }
            ObjRef::Ptr(pos) => {
                if let Some(cached) = self.expr_cache.get(&pos) {
                    return Ok(cached.clone());
                }
                let tag = self.region.obj_tag(pos);
                let expr = match tag {
                    0 => {
                        // bvar(nat)
                        let nat_ref = self.region.ctor_obj_field(pos, 0);
                        Expr::bvar(self.deser_nat(nat_ref))
                    }
                    1 => {
                        // fvar(FVarId = Name)
                        let name_ref = self.region.ctor_obj_field(pos, 0);
                        Expr::fvar(self.deser_name(name_ref))
                    }
                    2 => {
                        // mvar(MVarId = Name)
                        let name_ref = self.region.ctor_obj_field(pos, 0);
                        Expr::mvar(self.deser_name(name_ref))
                    }
                    3 => {
                        // sort(level)
                        let level_ref = self.region.ctor_obj_field(pos, 0);
                        Expr::sort(self.deser_level(level_ref))
                    }
                    4 => {
                        // const(name, List Level)
                        let name_ref = self.region.ctor_obj_field(pos, 0);
                        let levels_ref = self.region.ctor_obj_field(pos, 1);
                        let name = self.deser_name(name_ref);
                        let levels = self.deser_list(levels_ref, |d, r| Ok(d.deser_level(r)))?;
                        Expr::const_(name, levels)
                    }
                    // App, Lam, ForallE, LetE use iterative spine/chain unrolling
                    // to avoid stack overflow on deeply nested expressions.
                    5 => return self.deser_app_spine(pos),
                    6 => return self.deser_lam_chain(pos),
                    7 => return self.deser_forall_chain(pos),
                    8 => return self.deser_let_chain(pos),
                    9 => {
                        // lit(Literal)
                        let lit_ref = self.region.ctor_obj_field(pos, 0);
                        let lit = self.deser_literal(lit_ref)?;
                        Expr::lit(lit)
                    }
                    10 => {
                        // mdata(MData, expr)
                        let mdata_ref = self.region.ctor_obj_field(pos, 0);
                        let expr_ref = self.region.ctor_obj_field(pos, 1);
                        let mdata = self.deser_mdata(mdata_ref)?;
                        let inner = self.deser_expr(expr_ref)?;
                        Expr::mdata(mdata, inner)
                    }
                    11 => {
                        // proj(typeName, idx, struct)
                        let name_ref = self.region.ctor_obj_field(pos, 0);
                        let idx_ref = self.region.ctor_obj_field(pos, 1);
                        let struct_ref = self.region.ctor_obj_field(pos, 2);
                        let name = self.deser_name(name_ref);
                        let idx = self.deser_nat(idx_ref);
                        let s = self.deser_expr(struct_ref)?;
                        Expr::proj(name, idx, s)
                    }
                    _ => {
                        return Err(OleanError::Deserialize(format!(
                            "unknown Expr tag: {}",
                            tag
                        )));
                    }
                };
                self.expr_cache.insert(pos, expr.clone());
                Ok(expr)
            }
        }
    }

    /// Iteratively deserialize a left-recursive App spine: App(App(App(f, a1), a2), a3).
    /// Follows the fn (left) child iteratively, then rebuilds from the leaf outward.
    fn deser_app_spine(&mut self, start_pos: usize) -> OleanResult<Expr> {
        // Collect (pos, arg_ref) from outermost to innermost App node.
        let mut spine: Vec<(usize, ObjRef)> = Vec::new();
        let mut current_pos = start_pos;
        let mut leaf_fn_ref: ObjRef;

        loop {
            let fn_ref = self.region.ctor_obj_field(current_pos, 0);
            let arg_ref = self.region.ctor_obj_field(current_pos, 1);
            spine.push((current_pos, arg_ref));

            leaf_fn_ref = fn_ref;
            match fn_ref {
                ObjRef::Ptr(p)
                    if !self.expr_cache.contains_key(&p) && self.region.obj_tag(p) == 5 =>
                {
                    current_pos = p;
                }
                _ => break,
            }
        }

        // Deserialize the leaf fn (not an App, so bounded recursion depth).
        let mut result = self.deser_expr(leaf_fn_ref)?;

        // Rebuild from innermost to outermost, caching each intermediate node.
        for &(pos, arg_ref) in spine.iter().rev() {
            let arg = self.deser_expr(arg_ref)?;
            result = Expr::app(result, arg);
            self.expr_cache.insert(pos, result.clone());
        }

        Ok(result)
    }

    /// Iteratively deserialize a right-recursive Lam chain: λa. λb. λc. body.
    /// Follows the body (right) child iteratively, then rebuilds from the leaf outward.
    fn deser_lam_chain(&mut self, start_pos: usize) -> OleanResult<Expr> {
        struct Binder {
            pos: usize,
            name: Name,
            type_ref: ObjRef,
            bi: u8,
        }
        let mut binders: Vec<Binder> = Vec::new();
        let mut current_pos = start_pos;
        let mut leaf_body_ref: ObjRef;

        loop {
            let name_ref = self.region.ctor_obj_field(current_pos, 0);
            let type_ref = self.region.ctor_obj_field(current_pos, 1);
            let body_ref = self.region.ctor_obj_field(current_pos, 2);
            let bi = self.region.ctor_scalar_u8(current_pos, 3, 0);
            let name = self.deser_name(name_ref);

            binders.push(Binder {
                pos: current_pos,
                name,
                type_ref,
                bi,
            });

            leaf_body_ref = body_ref;
            match body_ref {
                ObjRef::Ptr(p)
                    if !self.expr_cache.contains_key(&p) && self.region.obj_tag(p) == 6 =>
                {
                    current_pos = p;
                }
                _ => break,
            }
        }

        // Deserialize the innermost body (not a Lam).
        let mut result = self.deser_expr(leaf_body_ref)?;

        // Rebuild from innermost to outermost.
        for binder in binders.into_iter().rev() {
            let ty = self.deser_expr(binder.type_ref)?;
            result = Expr::lam(binder.name, ty, result, deser_binder_info(binder.bi));
            self.expr_cache.insert(binder.pos, result.clone());
        }

        Ok(result)
    }

    /// Iteratively deserialize a right-recursive ForallE chain: ∀a. ∀b. body.
    fn deser_forall_chain(&mut self, start_pos: usize) -> OleanResult<Expr> {
        struct Binder {
            pos: usize,
            name: Name,
            type_ref: ObjRef,
            bi: u8,
        }
        let mut binders: Vec<Binder> = Vec::new();
        let mut current_pos = start_pos;
        let mut leaf_body_ref: ObjRef;

        loop {
            let name_ref = self.region.ctor_obj_field(current_pos, 0);
            let type_ref = self.region.ctor_obj_field(current_pos, 1);
            let body_ref = self.region.ctor_obj_field(current_pos, 2);
            let bi = self.region.ctor_scalar_u8(current_pos, 3, 0);
            let name = self.deser_name(name_ref);

            binders.push(Binder {
                pos: current_pos,
                name,
                type_ref,
                bi,
            });

            leaf_body_ref = body_ref;
            match body_ref {
                ObjRef::Ptr(p)
                    if !self.expr_cache.contains_key(&p) && self.region.obj_tag(p) == 7 =>
                {
                    current_pos = p;
                }
                _ => break,
            }
        }

        let mut result = self.deser_expr(leaf_body_ref)?;

        for binder in binders.into_iter().rev() {
            let ty = self.deser_expr(binder.type_ref)?;
            result = Expr::forall_e(binder.name, ty, result, deser_binder_info(binder.bi));
            self.expr_cache.insert(binder.pos, result.clone());
        }

        Ok(result)
    }

    /// Iteratively deserialize a right-recursive LetE chain: let a := x in let b := y in body.
    fn deser_let_chain(&mut self, start_pos: usize) -> OleanResult<Expr> {
        struct LetBinder {
            pos: usize,
            name: Name,
            type_ref: ObjRef,
            val_ref: ObjRef,
            nondep: bool,
        }
        let mut binders: Vec<LetBinder> = Vec::new();
        let mut current_pos = start_pos;
        let mut leaf_body_ref: ObjRef;

        loop {
            let name_ref = self.region.ctor_obj_field(current_pos, 0);
            let type_ref = self.region.ctor_obj_field(current_pos, 1);
            let val_ref = self.region.ctor_obj_field(current_pos, 2);
            let body_ref = self.region.ctor_obj_field(current_pos, 3);
            let nondep = self.region.ctor_scalar_u8(current_pos, 4, 0) != 0;
            let name = self.deser_name(name_ref);

            binders.push(LetBinder {
                pos: current_pos,
                name,
                type_ref,
                val_ref,
                nondep,
            });

            leaf_body_ref = body_ref;
            match body_ref {
                ObjRef::Ptr(p)
                    if !self.expr_cache.contains_key(&p) && self.region.obj_tag(p) == 8 =>
                {
                    current_pos = p;
                }
                _ => break,
            }
        }

        let mut result = self.deser_expr(leaf_body_ref)?;

        for binder in binders.into_iter().rev() {
            let ty = self.deser_expr(binder.type_ref)?;
            let val = self.deser_expr(binder.val_ref)?;
            result = Expr::let_e(binder.name, ty, val, result, binder.nondep);
            self.expr_cache.insert(binder.pos, result.clone());
        }

        Ok(result)
    }

    // ─── Literal ────────────────────────────────────────────────────────
    // Tag 0: natVal(nat) — 1 obj field
    // Tag 1: strVal(string) — 1 obj field

    fn deser_literal(&mut self, obj: ObjRef) -> OleanResult<Literal> {
        let pos = self.expect_ptr(obj, "Literal")?;
        let tag = self.region.obj_tag(pos);
        match tag {
            0 => {
                let nat_ref = self.region.ctor_obj_field(pos, 0);
                let n = self.deser_nat(nat_ref);
                Ok(Literal::Nat(num_bigint::BigUint::from(n)))
            }
            1 => {
                let str_ref = self.region.ctor_obj_field(pos, 0);
                let s = self.deser_string_obj(str_ref);
                Ok(Literal::Str(s))
            }
            _ => Err(OleanError::Deserialize(format!(
                "unknown Literal tag: {}",
                tag
            ))),
        }
    }

    // ─── MData (= KVMap = List (Name × DataValue)) ─────────────────────
    // KVMap is a single-field structure → unboxed → it's just a List

    fn deser_mdata(&mut self, obj: ObjRef) -> OleanResult<MData> {
        let entries: Vec<(Name, MDataValue)> =
            self.deser_list(obj, |d, pair_ref| d.deser_mdata_entry(pair_ref))?;
        Ok(entries)
    }

    fn deser_mdata_entry(&mut self, obj: ObjRef) -> OleanResult<(Name, MDataValue)> {
        // Prod (Name × DataValue) — structure tag 0, 2 obj fields
        let pos = self.expect_ptr(obj, "KVMap entry")?;
        let name_ref = self.region.ctor_obj_field(pos, 0);
        let val_ref = self.region.ctor_obj_field(pos, 1);
        let name = self.deser_name(name_ref);
        let val = self.deser_data_value(val_ref)?;
        Ok((name, val))
    }

    // DataValue tags: 0=ofString, 1=ofBool, 2=ofName, 3=ofNat, 4=ofInt, 5=ofSyntax
    fn deser_data_value(&mut self, obj: ObjRef) -> OleanResult<MDataValue> {
        match obj {
            ObjRef::Scalar(tag) => {
                // Bool constructors: ofBool(false) or ofBool(true) — but these would
                // have a sub-object. This case shouldn't happen for DataValue.
                Ok(MDataValue::Nat(tag))
            }
            ObjRef::Ptr(pos) => {
                let tag = self.region.obj_tag(pos);
                match tag {
                    0 => {
                        // ofString
                        let s_ref = self.region.ctor_obj_field(pos, 0);
                        Ok(MDataValue::String(self.deser_string_obj(s_ref)))
                    }
                    1 => {
                        // ofBool
                        let b_ref = self.region.ctor_obj_field(pos, 0);
                        Ok(MDataValue::Bool(self.deser_bool(b_ref)))
                    }
                    2 => {
                        // ofName
                        let n_ref = self.region.ctor_obj_field(pos, 0);
                        Ok(MDataValue::Name(self.deser_name(n_ref)))
                    }
                    3 => {
                        // ofNat
                        let n_ref = self.region.ctor_obj_field(pos, 0);
                        Ok(MDataValue::Nat(self.deser_nat(n_ref)))
                    }
                    _ => {
                        // ofInt (4) and ofSyntax (5) — skip
                        Ok(MDataValue::Nat(0))
                    }
                }
            }
            ObjRef::Null => Ok(MDataValue::Nat(0)),
        }
    }

    // ─── ConstantInfo ───────────────────────────────────────────────────
    // Tag 0: axiomInfo(AxiomVal)
    // Tag 1: defnInfo(DefinitionVal)
    // Tag 2: thmInfo(TheoremVal)
    // Tag 3: opaqueInfo(OpaqueVal)
    // Tag 4: quotInfo(QuotVal)
    // Tag 5: inductInfo(InductiveVal)
    // Tag 6: ctorInfo(ConstructorVal)
    // Tag 7: recInfo(RecursorVal)

    fn deser_constant_info(&mut self, obj: ObjRef) -> OleanResult<ConstantInfo> {
        let pos = self.expect_ptr(obj, "ConstantInfo")?;
        let tag = self.region.obj_tag(pos);
        // Each variant wraps a single Val structure (1 obj field)
        let val_ref = self.region.ctor_obj_field(pos, 0);
        let val_pos = self.expect_ptr(val_ref, "ConstantInfo.val")?;

        match tag {
            0 => self.deser_axiom_val(val_pos),
            1 => self.deser_definition_val(val_pos),
            2 => self.deser_theorem_val(val_pos),
            3 => self.deser_opaque_val(val_pos),
            4 => self.deser_quot_val(val_pos),
            5 => self.deser_inductive_val(val_pos),
            6 => self.deser_constructor_val(val_pos),
            7 => self.deser_recursor_val(val_pos),
            _ => Err(OleanError::Deserialize(format!(
                "unknown ConstantInfo tag: {}",
                tag
            ))),
        }
    }

    // ─── ConstantVal (base structure) ───────────────────────────────────
    // Structure tag 0: obj[name: Name, levelParams: List Name, type: Expr]

    fn deser_constant_val(&mut self, pos: usize) -> OleanResult<(Name, Vec<Name>, Expr)> {
        let name_ref = self.region.ctor_obj_field(pos, 0);
        let params_ref = self.region.ctor_obj_field(pos, 1);
        let type_ref = self.region.ctor_obj_field(pos, 2);

        let name = self.deser_name(name_ref);
        let level_params = self.deser_list(params_ref, |d, r| Ok(d.deser_name(r)))?;
        let type_ = self.deser_expr(type_ref)?;

        Ok((name, level_params, type_))
    }

    // AxiomVal extends ConstantVal: obj[toConstantVal], scalar[isUnsafe: u8]
    fn deser_axiom_val(&mut self, pos: usize) -> OleanResult<ConstantInfo> {
        let cv_ref = self.region.ctor_obj_field(pos, 0);
        let cv_pos = self.expect_ptr(cv_ref, "AxiomVal.toConstantVal")?;
        let (name, level_params, type_) = self.deser_constant_val(cv_pos)?;
        let is_unsafe = self.region.ctor_scalar_u8(pos, 1, 0) != 0;
        Ok(ConstantInfo::Axiom {
            name,
            level_params,
            type_,
            is_unsafe,
        })
    }

    // DefinitionVal extends ConstantVal:
    //   obj[toConstantVal, value: Expr, hints: ReducibilityHints, safety: DefinitionSafety, all: List Name]
    //   No scalar fields (safety and hints are boxed enums)
    fn deser_definition_val(&mut self, pos: usize) -> OleanResult<ConstantInfo> {
        let cv_ref = self.region.ctor_obj_field(pos, 0);
        let value_ref = self.region.ctor_obj_field(pos, 1);
        let hints_ref = self.region.ctor_obj_field(pos, 2);
        let safety_ref = self.region.ctor_obj_field(pos, 3);
        // obj 4: all (List Name) — we read but don't store

        let cv_pos = self.expect_ptr(cv_ref, "DefinitionVal.toConstantVal")?;
        let (name, level_params, type_) = self.deser_constant_val(cv_pos)?;
        let value = self.deser_expr(value_ref)?;
        let hints = self.deser_reducibility_hints(hints_ref)?;
        let safety = deser_definition_safety(safety_ref);

        Ok(ConstantInfo::Definition {
            name,
            level_params,
            type_,
            value,
            hints,
            safety,
        })
    }

    // TheoremVal extends ConstantVal: obj[toConstantVal, value: Expr, all: List Name]
    fn deser_theorem_val(&mut self, pos: usize) -> OleanResult<ConstantInfo> {
        let cv_ref = self.region.ctor_obj_field(pos, 0);
        let value_ref = self.region.ctor_obj_field(pos, 1);
        // obj 2: all — skip

        let cv_pos = self.expect_ptr(cv_ref, "TheoremVal.toConstantVal")?;
        let (name, level_params, type_) = self.deser_constant_val(cv_pos)?;
        let value = self.deser_expr(value_ref)?;

        Ok(ConstantInfo::Theorem {
            name,
            level_params,
            type_,
            value,
        })
    }

    // OpaqueVal extends ConstantVal: obj[toConstantVal, value: Expr, all: List Name], scalar[isUnsafe: u8]
    fn deser_opaque_val(&mut self, pos: usize) -> OleanResult<ConstantInfo> {
        let cv_ref = self.region.ctor_obj_field(pos, 0);
        let value_ref = self.region.ctor_obj_field(pos, 1);
        // obj 2: all — skip
        let is_unsafe = self.region.ctor_scalar_u8(pos, 3, 0) != 0;

        let cv_pos = self.expect_ptr(cv_ref, "OpaqueVal.toConstantVal")?;
        let (name, level_params, type_) = self.deser_constant_val(cv_pos)?;
        let value = self.deser_expr(value_ref)?;

        Ok(ConstantInfo::Opaque {
            name,
            level_params,
            type_,
            value,
            is_unsafe,
        })
    }

    // QuotVal extends ConstantVal: obj[toConstantVal, kind: QuotKind]
    fn deser_quot_val(&mut self, pos: usize) -> OleanResult<ConstantInfo> {
        let cv_ref = self.region.ctor_obj_field(pos, 0);
        let kind_ref = self.region.ctor_obj_field(pos, 1);

        let cv_pos = self.expect_ptr(cv_ref, "QuotVal.toConstantVal")?;
        let (name, level_params, type_) = self.deser_constant_val(cv_pos)?;
        let kind = deser_quot_kind(kind_ref);

        Ok(ConstantInfo::Quot {
            name,
            level_params,
            type_,
            kind,
        })
    }

    // InductiveVal extends ConstantVal:
    //   obj[toConstantVal, all: List Name, ctors: List Name],
    //   scalar[numParams: Nat, numIndices: Nat, numNested: Nat],  — these are actually obj fields!
    //   Wait: Nat is boxed, so numParams etc are obj fields, and Bool is scalar.
    //   Fields: toConstantVal, numParams, numIndices, all, ctors, numNested — all obj
    //           isRec, isUnsafe, isReflexive — scalar u8
    fn deser_inductive_val(&mut self, pos: usize) -> OleanResult<ConstantInfo> {
        // InductiveVal extends ConstantVal with:
        //   numParams : Nat, numIndices : Nat, all : List Name, ctors : List Name,
        //   numNested : Nat, isRec : Bool, isUnsafe : Bool, isReflexive : Bool
        // Object fields: toConstantVal, numParams, numIndices, all, ctors, numNested (6 obj)
        // Scalar fields: isRec, isUnsafe, isReflexive (3 × u8)
        let cv_ref = self.region.ctor_obj_field(pos, 0);
        let num_params_ref = self.region.ctor_obj_field(pos, 1);
        let num_indices_ref = self.region.ctor_obj_field(pos, 2);
        let all_ref = self.region.ctor_obj_field(pos, 3);
        let ctors_ref = self.region.ctor_obj_field(pos, 4);
        let num_nested_ref = self.region.ctor_obj_field(pos, 5);
        let is_rec = self.region.ctor_scalar_u8(pos, 6, 0) != 0;
        let is_unsafe = self.region.ctor_scalar_u8(pos, 6, 1) != 0;
        let is_reflexive = self.region.ctor_scalar_u8(pos, 6, 2) != 0;

        let cv_pos = self.expect_ptr(cv_ref, "InductiveVal.toConstantVal")?;
        let (name, level_params, type_) = self.deser_constant_val(cv_pos)?;
        let num_params = self.deser_nat(num_params_ref) as u32;
        let num_indices = self.deser_nat(num_indices_ref) as u32;
        let all = self.deser_list(all_ref, |d, r| Ok(d.deser_name(r)))?;
        let ctors = self.deser_list(ctors_ref, |d, r| Ok(d.deser_name(r)))?;
        let num_nested = self.deser_nat(num_nested_ref) as u32;

        Ok(ConstantInfo::Inductive {
            name,
            level_params,
            type_,
            num_params,
            num_indices,
            all,
            ctors,
            num_nested,
            is_rec,
            is_unsafe,
            is_reflexive,
        })
    }

    // ConstructorVal extends ConstantVal:
    //   obj[toConstantVal, induct: Name, cidx: Nat, numParams: Nat, numFields: Nat]
    //   scalar[isUnsafe: u8]
    fn deser_constructor_val(&mut self, pos: usize) -> OleanResult<ConstantInfo> {
        let cv_ref = self.region.ctor_obj_field(pos, 0);
        let induct_ref = self.region.ctor_obj_field(pos, 1);
        let cidx_ref = self.region.ctor_obj_field(pos, 2);
        let num_params_ref = self.region.ctor_obj_field(pos, 3);
        let num_fields_ref = self.region.ctor_obj_field(pos, 4);
        let is_unsafe = self.region.ctor_scalar_u8(pos, 5, 0) != 0;

        let cv_pos = self.expect_ptr(cv_ref, "ConstructorVal.toConstantVal")?;
        let (name, level_params, type_) = self.deser_constant_val(cv_pos)?;
        let induct_name = self.deser_name(induct_ref);
        let ctor_idx = self.deser_nat(cidx_ref) as u32;
        let num_params = self.deser_nat(num_params_ref) as u32;
        let num_fields = self.deser_nat(num_fields_ref) as u32;

        Ok(ConstantInfo::Constructor {
            name,
            level_params,
            type_,
            induct_name,
            ctor_idx,
            num_params,
            num_fields,
            is_unsafe,
        })
    }

    // RecursorVal extends ConstantVal:
    //   obj[toConstantVal, all: List Name, numParams: Nat, numIndices: Nat,
    //       numMotives: Nat, numMinors: Nat, rules: List RecursorRule]
    //   scalar[k: u8, isUnsafe: u8]
    fn deser_recursor_val(&mut self, pos: usize) -> OleanResult<ConstantInfo> {
        let cv_ref = self.region.ctor_obj_field(pos, 0);
        let all_ref = self.region.ctor_obj_field(pos, 1);
        let num_params_ref = self.region.ctor_obj_field(pos, 2);
        let num_indices_ref = self.region.ctor_obj_field(pos, 3);
        let num_motives_ref = self.region.ctor_obj_field(pos, 4);
        let num_minors_ref = self.region.ctor_obj_field(pos, 5);
        let rules_ref = self.region.ctor_obj_field(pos, 6);
        let is_k = self.region.ctor_scalar_u8(pos, 7, 0) != 0;
        let is_unsafe = self.region.ctor_scalar_u8(pos, 7, 1) != 0;

        let cv_pos = self.expect_ptr(cv_ref, "RecursorVal.toConstantVal")?;
        let (name, level_params, type_) = self.deser_constant_val(cv_pos)?;
        let _all = self.deser_list(all_ref, |d, r| Ok(d.deser_name(r)))?;
        let num_params = self.deser_nat(num_params_ref) as u32;
        let num_indices = self.deser_nat(num_indices_ref) as u32;
        let num_motives = self.deser_nat(num_motives_ref) as u32;
        let num_minors = self.deser_nat(num_minors_ref) as u32;
        let rules = self.deser_list(rules_ref, |d, r| d.deser_recursor_rule(r))?;

        Ok(ConstantInfo::Recursor {
            name,
            level_params,
            type_,
            all: _all,
            num_params,
            num_indices,
            num_motives,
            num_minors,
            rules,
            is_k,
            is_unsafe,
        })
    }

    // RecursorRule: structure tag 0, obj[ctor: Name, nfields: Nat, rhs: Expr]
    fn deser_recursor_rule(&mut self, obj: ObjRef) -> OleanResult<RecursorRule> {
        let pos = self.expect_ptr(obj, "RecursorRule")?;
        let ctor_ref = self.region.ctor_obj_field(pos, 0);
        let nfields_ref = self.region.ctor_obj_field(pos, 1);
        let rhs_ref = self.region.ctor_obj_field(pos, 2);

        let ctor_name = self.deser_name(ctor_ref);
        let num_fields = self.deser_nat(nfields_ref) as u32;
        let rhs = self.deser_expr(rhs_ref)?;

        Ok(RecursorRule {
            ctor_name,
            num_fields,
            rhs,
        })
    }

    // ─── ReducibilityHints ──────────────────────────────────────────────
    // Tag 0: opaque (no fields) → Scalar(0)
    // Tag 1: abbrev (no fields) → Scalar(1)
    // Tag 2: regular(height: UInt32) → 0 obj fields, 4 bytes scalar

    fn deser_reducibility_hints(&mut self, obj: ObjRef) -> OleanResult<ReducibilityHints> {
        match obj {
            ObjRef::Scalar(0) => Ok(ReducibilityHints::Opaque),
            ObjRef::Scalar(1) => Ok(ReducibilityHints::Abbreviation),
            ObjRef::Scalar(_) => Ok(ReducibilityHints::Opaque),
            ObjRef::Ptr(pos) => {
                let tag = self.region.obj_tag(pos);
                match tag {
                    0 => Ok(ReducibilityHints::Opaque),
                    1 => Ok(ReducibilityHints::Abbreviation),
                    2 => {
                        let height = self.region.ctor_scalar_u32(pos, 0, 0);
                        Ok(ReducibilityHints::Regular(height))
                    }
                    _ => Ok(ReducibilityHints::Opaque),
                }
            }
            ObjRef::Null => Ok(ReducibilityHints::Opaque),
        }
    }

    // ─── Generic helpers ────────────────────────────────────────────────

    /// Read a Lean Bool: Scalar(0) = false, Scalar(1) = true
    fn deser_bool(&self, obj: ObjRef) -> bool {
        matches!(obj, ObjRef::Scalar(1))
    }

    /// Read a Nat value (small = scalar, large = MPZ object).
    fn deser_nat(&self, obj: ObjRef) -> u64 {
        match obj {
            ObjRef::Scalar(n) => n,
            ObjRef::Ptr(pos) => {
                let tag = self.region.obj_tag(pos);
                if tag == LEAN_TAG_MPZ {
                    self.region.mpz_to_u64(pos).unwrap_or(0)
                } else {
                    0
                }
            }
            ObjRef::Null => 0,
        }
    }

    /// Read a String object (tag 249).
    fn deser_string_obj(&self, obj: ObjRef) -> String {
        match obj {
            ObjRef::Ptr(pos) => {
                let tag = self.region.obj_tag(pos);
                if tag == LEAN_TAG_STRING {
                    self.region.string_value(pos).unwrap_or("").to_string()
                } else {
                    String::new()
                }
            }
            _ => String::new(),
        }
    }

    /// Deserialize a Lean List as Vec<T>.
    /// List.nil = Scalar(0), List.cons = constructor tag 1 with [head, tail].
    fn deser_list<T>(
        &mut self,
        obj: ObjRef,
        f: fn(&mut Self, ObjRef) -> OleanResult<T>,
    ) -> OleanResult<Vec<T>> {
        let mut result = Vec::new();
        let mut current = obj;
        loop {
            match current {
                ObjRef::Null | ObjRef::Scalar(0) => break,
                ObjRef::Scalar(_) => break,
                ObjRef::Ptr(pos) => {
                    let tag = self.region.obj_tag(pos);
                    if tag == 0 {
                        // nil
                        break;
                    } else if tag == 1 {
                        // cons(head, tail)
                        let head_ref = self.region.ctor_obj_field(pos, 0);
                        let tail_ref = self.region.ctor_obj_field(pos, 1);
                        result.push(f(self, head_ref)?);
                        current = tail_ref;
                    } else {
                        break;
                    }
                }
            }
        }
        Ok(result)
    }

    /// Deserialize a Lean Array (tag 246) as Vec<T>.
    fn deser_array<T>(
        &mut self,
        obj: ObjRef,
        f: fn(&mut Self, ObjRef) -> OleanResult<T>,
    ) -> OleanResult<Vec<T>> {
        let pos = match obj {
            ObjRef::Ptr(pos) => pos,
            _ => return Ok(Vec::new()),
        };

        let tag = self.region.obj_tag(pos);
        if tag != LEAN_TAG_ARRAY {
            return Err(OleanError::Deserialize(format!(
                "expected Array (tag {}), got tag {}",
                LEAN_TAG_ARRAY, tag
            )));
        }

        let size = self.region.array_size(pos);
        let mut result = Vec::with_capacity(size);
        for i in 0..size {
            let elem = self.region.array_elem(pos, i);
            result.push(f(self, elem)?);
        }
        Ok(result)
    }

    /// Expect an ObjRef to be a Ptr, returning the position.
    fn expect_ptr(&self, obj: ObjRef, context: &str) -> OleanResult<usize> {
        match obj {
            ObjRef::Ptr(pos) => Ok(pos),
            _ => Err(OleanError::Deserialize(format!(
                "expected object pointer for {}, got {:?}",
                context, obj
            ))),
        }
    }
}

// ─── Stateless deserialization helpers ───────────────────────────────────

fn deser_binder_info(b: u8) -> BinderInfo {
    match b {
        0 => BinderInfo::Default,
        1 => BinderInfo::Implicit,
        2 => BinderInfo::StrictImplicit,
        3 => BinderInfo::InstImplicit,
        _ => BinderInfo::Default,
    }
}

fn deser_definition_safety(obj: ObjRef) -> DefinitionSafety {
    match obj {
        ObjRef::Scalar(0) => DefinitionSafety::Unsafe,
        ObjRef::Scalar(1) => DefinitionSafety::Safe,
        ObjRef::Scalar(2) => DefinitionSafety::Partial,
        _ => DefinitionSafety::Safe,
    }
}

fn deser_quot_kind(obj: ObjRef) -> QuotKind {
    match obj {
        ObjRef::Scalar(0) => QuotKind::Type,
        ObjRef::Scalar(1) => QuotKind::Mk,
        ObjRef::Scalar(2) => QuotKind::Lift,
        ObjRef::Scalar(3) => QuotKind::Ind,
        _ => QuotKind::Type,
    }
}
