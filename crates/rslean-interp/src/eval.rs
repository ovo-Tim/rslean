use rslean_expr::{ConstantInfo, Expr, ExprKind, Literal};
use rslean_kernel::Environment;
use rslean_level::Level;
use rslean_name::Name;
use rustc_hash::FxHashMap;
use std::sync::Arc;

use crate::builtins::BuiltinFn;
use crate::env::LocalEnv;
use crate::error::{InterpError, InterpResult};
use crate::iota;
use crate::value::{FuncRef, Value};

const MAX_EVAL_DEPTH: u32 = 256;

/// The tree-walking interpreter for Lean 4 kernel expressions.
pub struct Interpreter {
    env: Environment,
    builtins: FxHashMap<Name, BuiltinFn>,
    const_cache: FxHashMap<Name, Value>,
    eval_depth: u32,
}

impl Interpreter {
    pub fn new(env: Environment) -> Self {
        let mut builtins = FxHashMap::default();
        crate::builtins::register_builtins(&mut builtins);
        Interpreter {
            env,
            builtins,
            const_cache: FxHashMap::default(),
            eval_depth: 0,
        }
    }

    pub fn env(&self) -> &Environment {
        &self.env
    }

    /// Evaluate an expression to a value.
    pub fn eval(&mut self, expr: &Expr, local_env: &LocalEnv) -> InterpResult<Value> {
        self.eval_depth += 1;
        if self.eval_depth > MAX_EVAL_DEPTH {
            self.eval_depth -= 1;
            return Err(InterpError::StackOverflow(MAX_EVAL_DEPTH));
        }
        let result = self.eval_inner(expr, local_env);
        self.eval_depth -= 1;
        result
    }

    fn eval_inner(&mut self, expr: &Expr, local_env: &LocalEnv) -> InterpResult<Value> {
        match expr.kind() {
            ExprKind::Lit(lit) => self.eval_lit(lit),
            ExprKind::BVar(idx) => local_env.lookup(*idx).cloned(),
            ExprKind::Lam(_name, _ty, body, _bi) => {
                Ok(Value::Closure {
                    func: FuncRef::Lambda(body.clone(), local_env.clone()),
                    captured: vec![],
                    remaining_arity: 1,
                })
            }
            ExprKind::LetE(_name, _ty, val, body, _non_dep) => {
                let v = self.eval(val, local_env)?;
                let new_env = local_env.push(v);
                self.eval(body, &new_env)
            }
            ExprKind::App(f, a) => {
                let fv = self.eval(f, local_env)?;
                let av = self.eval(a, local_env)?;
                self.apply(fv, av)
            }
            ExprKind::Const(name, levels) => {
                self.eval_const(name, levels)
            }
            ExprKind::ForallE(..) | ExprKind::Sort(_) | ExprKind::MVar(_) => {
                Ok(Value::Erased)
            }
            ExprKind::MData(_md, e) => {
                self.eval(e, local_env)
            }
            ExprKind::Proj(struct_name, idx, e) => {
                let v = self.eval(e, local_env)?;
                self.eval_proj(struct_name, *idx, v)
            }
            ExprKind::FVar(_) => {
                Ok(Value::KernelExpr(expr.clone()))
            }
        }
    }

    fn eval_lit(&self, lit: &Literal) -> InterpResult<Value> {
        match lit {
            Literal::Nat(n) => Ok(Value::Nat(Arc::new(n.clone()))),
            Literal::Str(s) => Ok(Value::String(Arc::from(s.as_str()))),
        }
    }

    /// Evaluate a constant reference.
    pub(crate) fn eval_const(&mut self, name: &Name, levels: &[Level]) -> InterpResult<Value> {
        // Check builtin registry first
        if self.builtins.contains_key(name) {
            return self.make_builtin_closure(name);
        }

        // Check cache (only for level-monomorphic constants)
        if levels.is_empty() || levels.iter().all(|l| l.is_explicit()) {
            if let Some(cached) = self.const_cache.get(name) {
                return Ok(cached.clone());
            }
        }

        let info = self.env.get(name)?.clone();
        let val = self.eval_const_info(&info, levels)?;

        // Cache if appropriate
        if levels.is_empty() || levels.iter().all(|l| l.is_explicit()) {
            self.const_cache.insert(name.clone(), val.clone());
        }

        Ok(val)
    }

    fn eval_const_info(
        &mut self,
        info: &ConstantInfo,
        levels: &[Level],
    ) -> InterpResult<Value> {
        match info {
            ConstantInfo::Definition {
                level_params,
                value,
                ..
            }
            | ConstantInfo::Theorem {
                level_params,
                value,
                ..
            } => {
                let body = if !level_params.is_empty() && !levels.is_empty() {
                    value.instantiate_level_params(level_params, levels)
                } else {
                    value.clone()
                };
                self.eval(&body, &LocalEnv::new())
            }

            ConstantInfo::Constructor {
                name,
                ctor_idx,
                num_params,
                num_fields,
                ..
            } => {
                let total_arity = *num_params + *num_fields;
                if total_arity == 0 {
                    // Special-case Nat.zero
                    if name == &Name::from_str_parts("Nat.zero") {
                        return Ok(Value::nat_small(0));
                    }
                    Ok(Value::Ctor {
                        tag: *ctor_idx,
                        name: name.clone(),
                        fields: vec![],
                    })
                } else {
                    Ok(Value::Closure {
                        func: FuncRef::CtorFn {
                            name: name.clone(),
                            tag: *ctor_idx,
                            num_params: *num_params,
                            num_fields: *num_fields,
                        },
                        captured: vec![],
                        remaining_arity: total_arity,
                    })
                }
            }

            ConstantInfo::Recursor { name, .. } => {
                let total_arity = iota::recursor_total_arity(info);
                Ok(Value::Closure {
                    func: FuncRef::RecursorFn(name.clone(), levels.to_vec()),
                    captured: vec![],
                    remaining_arity: total_arity,
                })
            }

            ConstantInfo::Inductive { .. } => {
                Ok(Value::Erased)
            }

            ConstantInfo::Axiom { .. }
            | ConstantInfo::Opaque { .. }
            | ConstantInfo::Quot { .. } => Ok(Value::Erased),
        }
    }

    /// Determine arity of a builtin from its type in the environment, then create a closure.
    fn make_builtin_closure(&self, name: &Name) -> InterpResult<Value> {
        let arity = self.compute_arity_from_type(name);
        Ok(Value::Closure {
            func: FuncRef::Builtin(name.clone()),
            captured: vec![],
            remaining_arity: arity,
        })
    }

    /// Compute the arity of a constant by counting pi-binders in its type.
    /// Delta-reduces type aliases (ST, EST, EIO, IO, etc.) to expose hidden binders
    /// from monadic return types like `ST σ α = Void σ → ST.Out σ α`.
    fn compute_arity_from_type(&self, name: &Name) -> u32 {
        if let Some(info) = self.env.find(name) {
            let mut ty = info.type_().clone();
            let mut arity = 0u32;
            let mut reduction_fuel = 8u32;

            loop {
                // Count ForallE binders
                while let ExprKind::ForallE(_, _, body, _) = ty.kind() {
                    arity += 1;
                    ty = body.clone();
                }

                // Try to delta-reduce if the result type is an application of a
                // known type alias that hides additional binders.
                if reduction_fuel == 0 {
                    break;
                }
                reduction_fuel -= 1;

                if let Some(head_name) = self.get_app_head_const(&ty) {
                    if self.is_function_type_alias(&head_name) {
                        if let Some(reduced) = self.try_delta_reduce_type(&ty) {
                            ty = reduced;
                            continue;
                        }
                    }
                }
                break;
            }

            if arity == 0 { 1 } else { arity }
        } else {
            // Fallback: arity 1
            1
        }
    }

    /// Get the head constant name from a (possibly nested) application.
    fn get_app_head_const(&self, expr: &Expr) -> Option<Name> {
        let mut e = expr;
        loop {
            match e.kind() {
                ExprKind::App(f, _) => e = f,
                ExprKind::Const(name, _) => return Some(name.clone()),
                _ => return None,
            }
        }
    }

    /// Check if a name is a type alias that may hide function binders.
    /// These are types like ST, EST, EIO, BaseIO, IO that expand to
    /// something containing `→` (ForallE).
    fn is_function_type_alias(&self, name: &Name) -> bool {
        // Known Lean 4 monadic type aliases that expand to functions
        static ALIASES: &[&str] = &[
            "ST", "EST", "EIO", "BaseIO", "IO",
            "ST'", "EST'",
            "StateM", "ReaderT", "StateT", "ExceptT",
        ];
        let name_str = name.to_string();
        ALIASES.iter().any(|a| name_str == *a)
    }

    /// Try to delta-reduce a type expression by unfolding the head constant's definition.
    /// Returns the reduced type with arguments substituted, or None if it can't be reduced.
    fn try_delta_reduce_type(&self, expr: &Expr) -> Option<Expr> {
        // Collect application arguments
        let mut args = Vec::new();
        let mut e = expr;
        while let ExprKind::App(f, a) = e.kind() {
            args.push(a.clone());
            e = f;
        }
        args.reverse();

        let (name, levels) = match e.kind() {
            ExprKind::Const(name, levels) => (name, levels),
            _ => return None,
        };

        // Look up the definition
        let info = self.env.find(name)?;
        let (level_params, value) = match info {
            ConstantInfo::Definition { level_params, value, .. } => (level_params, value),
            _ => return None,
        };

        // Instantiate universe parameters
        let mut body = if !level_params.is_empty() && !levels.is_empty() {
            value.instantiate_level_params(level_params, levels)
        } else {
            value.clone()
        };

        // Apply arguments by stripping lambdas
        for arg in &args {
            match body.kind() {
                ExprKind::Lam(_, _, lam_body, _) => {
                    body = lam_body.instantiate(std::slice::from_ref(arg));
                }
                _ => return None,
            }
        }

        Some(body)
    }

    /// Apply a function value to an argument.
    pub(crate) fn apply(&mut self, func: Value, arg: Value) -> InterpResult<Value> {
        match func {
            Value::Closure {
                func: fref,
                mut captured,
                remaining_arity,
            } => {
                captured.push(arg);
                if remaining_arity <= 1 {
                    self.apply_fully(fref, captured)
                } else {
                    Ok(Value::Closure {
                        func: fref,
                        captured,
                        remaining_arity: remaining_arity - 1,
                    })
                }
            }
            // Applying to erased/type values — just erase
            Value::Erased => Ok(Value::Erased),
            // Applying to a KernelExpr — build an application expr
            Value::KernelExpr(e) => {
                // Can't reduce further, keep as KernelExpr
                Ok(Value::KernelExpr(e))
            }
            _ => Err(InterpError::TypeError(format!(
                "cannot apply non-function value: {:?}",
                func
            ))),
        }
    }

    /// Execute a fully-applied function.
    fn apply_fully(&mut self, fref: FuncRef, args: Vec<Value>) -> InterpResult<Value> {
        match fref {
            FuncRef::Lambda(body, env) => {
                // The lambda captured `env`, and we have exactly one new arg
                // But the arg is the last element of `args` after captured.
                // Actually for a lambda, remaining_arity is 1, so args has
                // all previously captured + the final arg.
                // But captured was empty for a fresh lambda.
                // The last arg is the lambda parameter.
                let arg = args.into_iter().last().unwrap();
                let new_env = env.push(arg);
                self.eval(&body, &new_env)
            }
            FuncRef::Definition(name, levels) => {
                let val = self.eval_const(&name, &levels)?;
                // Apply all captured args
                let mut result = val;
                for arg in args {
                    result = self.apply(result, arg)?;
                }
                Ok(result)
            }
            FuncRef::Builtin(name) => {
                let builtin_fn = self
                    .builtins
                    .get(&name)
                    .copied()
                    .ok_or_else(|| InterpError::UnknownConstant(name.clone()))?;
                builtin_fn(&args)
            }
            FuncRef::CtorFn {
                name,
                tag,
                num_params,
                num_fields,
            } => {
                // Skip params, take only fields
                let fields: Vec<Value> = args
                    .into_iter()
                    .skip(num_params as usize)
                    .take(num_fields as usize)
                    .collect();
                // Special-case Nat constructors to keep the Nat representation.
                if name == Name::from_str_parts("Nat.zero") {
                    return Ok(Value::nat_small(0));
                }
                if name == Name::from_str_parts("Nat.succ") {
                    if let Some(Value::Nat(n)) = fields.first() {
                        return Ok(Value::Nat(Arc::new(n.as_ref() + 1u32)));
                    }
                }
                Ok(Value::Ctor { tag, name, fields })
            }
            FuncRef::RecursorFn(name, levels) => {
                iota::apply_recursor(self, &name, &levels, args)
            }
        }
    }

    /// Evaluate a projection.
    fn eval_proj(&self, struct_name: &Name, idx: u64, val: Value) -> InterpResult<Value> {
        match val {
            Value::Ctor { fields, .. } => {
                fields.get(idx as usize).cloned().ok_or_else(|| {
                    InterpError::ProjOutOfRange {
                        struct_name: struct_name.clone(),
                        idx,
                        num_fields: fields.len(),
                    }
                })
            }
            Value::Erased => Ok(Value::Erased),
            _ => Err(InterpError::TypeError(format!(
                "projection {}.{} on non-constructor value: {:?}",
                struct_name, idx, val
            ))),
        }
    }
}
