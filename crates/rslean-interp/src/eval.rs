use rslean_expr::{ConstantInfo, Expr, ExprKind, Literal};
use rslean_kernel::Environment;
use rslean_level::Level;
use rslean_name::Name;
use rustc_hash::FxHashMap;
use std::sync::Arc;

use crate::builtins::{io_ok, BuiltinFn};
use crate::env::LocalEnv;
use crate::error::{InterpError, InterpResult};
use crate::iota;
use crate::value::{FuncRef, Value};

#[allow(dead_code)]
fn short_debug(v: &Value) -> String {
    match v {
        Value::Closure {
            func,
            remaining_arity,
            ..
        } => format!("Closure({:?}, arity={})", func, remaining_arity),
        Value::Ctor { tag, name, fields } => {
            format!("Ctor(tag={}, {}, {} fields)", tag, name, fields.len())
        }
        Value::Nat(n) => format!("Nat({})", n),
        Value::String(s) => format!("String({:?})", &s[..s.len().min(20)]),
        Value::Erased => "Erased".into(),
        Value::Ref(_) => "Ref(...)".into(),
        Value::Environment(_) => "Environment".into(),
        _ => format!("{:?}", v),
    }
}

#[allow(dead_code)]
fn expr_debug(e: &Expr) -> String {
    match e.kind() {
        ExprKind::BVar(i) => format!("BVar({})", i),
        ExprKind::Const(name, _) => format!("Const({})", name),
        ExprKind::App(f, a) => format!("App({}, {})", expr_debug(f), expr_debug(a)),
        ExprKind::Lam(name, _, _, _) => format!("Lam({},...)", name),
        ExprKind::Proj(sn, idx, _) => format!("Proj({}, {})", sn, idx),
        ExprKind::LetE(name, _, _, _, _) => format!("Let({},...)", name),
        _ => format!("{:?}", std::mem::discriminant(e.kind())),
    }
}

const DEFAULT_MAX_EVAL_DEPTH: u32 = 512;
const UNLIMITED_MAX_EVAL_DEPTH: u32 = u32::MAX;
const DEFAULT_MAX_STEPS: u64 = 100_000_000; // 100M steps default limit

/// The tree-walking interpreter for Lean 4 kernel expressions.
pub struct Interpreter {
    env: Environment,
    builtins: FxHashMap<Name, BuiltinFn>,
    const_cache: FxHashMap<Name, Value>,
    eval_depth: u32,
    /// Maximum eval recursion depth before aborting.
    pub max_eval_depth: u32,
    /// Total number of eval() calls since creation.
    pub total_steps: u64,
    /// Maximum number of eval() steps before aborting (0 = unlimited).
    pub max_steps: u64,
    /// When true, log const evaluations to stderr for diagnostics.
    pub trace_consts: bool,
    /// Counts of how many times each constant was evaluated (for diagnostics).
    pub const_eval_counts: FxHashMap<Name, u64>,
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
            max_eval_depth: DEFAULT_MAX_EVAL_DEPTH,
            total_steps: 0,
            max_steps: DEFAULT_MAX_STEPS,
            trace_consts: false,
            const_eval_counts: FxHashMap::default(),
        }
    }

    /// Create an interpreter with no step limit (for long-running operations).
    pub fn new_unlimited(env: Environment) -> Self {
        let mut interp = Self::new(env);
        interp.max_steps = 0;
        interp.max_eval_depth = UNLIMITED_MAX_EVAL_DEPTH;
        interp
    }

    pub fn env(&self) -> &Environment {
        &self.env
    }

    /// Get the top N most-evaluated constants (for diagnostics).
    pub fn top_evaluated_consts(&self, n: usize) -> Vec<(Name, u64)> {
        let mut counts: Vec<_> = self
            .const_eval_counts
            .iter()
            .map(|(k, v)| (k.clone(), *v))
            .collect();
        counts.sort_by(|a, b| b.1.cmp(&a.1));
        counts.truncate(n);
        counts
    }

    /// Detect compiler-generated auxiliary constants that should be erased.
    /// These include `_cstage2`, `_closed_N`, `_lambda_N`, `_neutral`, `_rarg`.
    pub(crate) fn is_compiler_aux(name: &Name) -> bool {
        if name.is_str() {
            let s = name.get_string();
            return s == "_cstage1" || s == "_cstage2" || s == "_neutral" || s == "_rarg";
        }
        if name.is_num() {
            let prefix = name.get_prefix();
            if prefix.is_str() {
                let s = prefix.get_string();
                return s == "_closed" || s == "_lambda";
            }
        }
        false
    }

    pub fn eval(&mut self, expr: &Expr, local_env: &LocalEnv) -> InterpResult<Value> {
        stacker::maybe_grow(32 * 1024, 2 * 1024 * 1024, || {
            self.total_steps += 1;
            if self.total_steps.is_multiple_of(1_000_000) {
                eprintln!(
                    "[PROGRESS] step {} depth={}",
                    self.total_steps, self.eval_depth
                );
            }
            if self.max_steps > 0 && self.total_steps > self.max_steps {
                return Err(InterpError::StepLimitExceeded(self.total_steps));
            }
            self.eval_depth += 1;
            if self.eval_depth > self.max_eval_depth {
                self.eval_depth -= 1;
                return Err(InterpError::StackOverflow(self.max_eval_depth));
            }
            let result = self.eval_inner(expr, local_env);
            self.eval_depth -= 1;
            result
        })
    }

    fn eval_inner(&mut self, expr: &Expr, local_env: &LocalEnv) -> InterpResult<Value> {
        let result = match expr.kind() {
            ExprKind::Lit(lit) => self.eval_lit(lit),
            ExprKind::BVar(idx) => local_env.lookup(*idx).cloned(),
            ExprKind::Lam(_name, _ty, body, _bi) => Ok(Value::Closure {
                func: FuncRef::Lambda(body.clone(), local_env.clone()),
                captured: vec![],
                remaining_arity: 1,
            }),
            ExprKind::LetE(_, _, _, _, _) => {
                // Flatten LetE chains to avoid O(N) native recursion depth.
                // Instead of recursing through LetE(_, _, v1, LetE(_, _, v2, ...)),
                // iterate and push bindings in a loop.
                let mut env = local_env.clone();
                let mut current = expr.clone();
                loop {
                    if let ExprKind::LetE(_, _, val, body, _) = current.kind() {
                        let val = val.clone();
                        let body = body.clone();
                        let v = self.eval(&val, &env)?;
                        env = env.push(v);
                        current = body;
                    } else {
                        return self.eval(&current, &env);
                    }
                }
            }
            ExprKind::App(_, _) => {
                // Flatten left-spine App chains: App(App(App(f, a1), a2), a3) -> head + [a1, a2, a3]
                // This converts O(N) recursive stack depth into O(1) for deeply nested applications.
                let mut args_exprs: Vec<Expr> = Vec::new();
                let mut head = expr.clone();
                while let ExprKind::App(f, a) = head.kind() {
                    args_exprs.push(a.clone());
                    head = f.clone();
                }
                args_exprs.reverse();

                // ite/dite short-circuit: evaluate only the taken branch
                if args_exprs.len() == 5 {
                    if let ExprKind::Const(name, _) = head.kind() {
                        let is_dite = if *name == Name::from_str_parts("ite") {
                            Some(false)
                        } else if *name == Name::from_str_parts("dite") {
                            Some(true)
                        } else {
                            None
                        };
                        if let Some(is_dite) = is_dite {
                            return self.eval_ite_short_circuit(
                                is_dite,
                                &args_exprs[2],
                                &args_exprs[3],
                                &args_exprs[4],
                                local_env,
                            );
                        }
                    }
                }

                let mut result = self.eval(&head, local_env)?;
                for arg_expr in &args_exprs {
                    let av = self.eval(arg_expr, local_env)?;
                    result = self.apply(result, av)?;
                }
                Ok(result)
            }
            ExprKind::Const(name, levels) => self.eval_const(name, levels),
            ExprKind::ForallE(..) | ExprKind::Sort(_) | ExprKind::MVar(_) => Ok(Value::Erased),
            ExprKind::MData(_md, e) => self.eval(e, local_env),
            ExprKind::Proj(struct_name, idx, e) => {
                let v = self.eval(e, local_env)?;
                self.eval_proj(struct_name, *idx, v)
            }
            ExprKind::FVar(_) => Ok(Value::KernelExpr(expr.clone())),
        };
        result
    }

    fn eval_lit(&self, lit: &Literal) -> InterpResult<Value> {
        match lit {
            Literal::Nat(n) => Ok(Value::Nat(Arc::new(n.clone()))),
            Literal::Str(s) => Ok(Value::String(Arc::from(s.as_str()))),
        }
    }

    pub(crate) fn eval_const(&mut self, name: &Name, levels: &[Level]) -> InterpResult<Value> {
        if self.builtins.contains_key(name) {
            return self.make_builtin_closure(name);
        }

        if Self::is_compiler_aux(name) {
            return Ok(Value::Erased);
        }

        if levels.is_empty() || levels.iter().all(|l| l.is_explicit()) {
            if let Some(cached) = self.const_cache.get(name) {
                return Ok(cached.clone());
            }
        }

        if self.trace_consts {
            *self.const_eval_counts.entry(name.clone()).or_insert(0) += 1;
            let count = self.const_eval_counts[name];
            if count <= 3 || count.is_multiple_of(1000) {
                eprintln!(
                    "[trace] eval_const {} (count={}, total_steps={})",
                    name, count, self.total_steps
                );
            }
        }

        let info = self.env.get(name)?.clone();
        let val = self.eval_const_info(&info, levels)?;

        // Cache if appropriate (limit cache size to avoid OOM)
        if (levels.is_empty() || levels.iter().all(|l| l.is_explicit()))
            && self.const_cache.len() < 10_000
        {
            self.const_cache.insert(name.clone(), val.clone());
        }

        Ok(val)
    }

    fn eval_const_info(&mut self, info: &ConstantInfo, levels: &[Level]) -> InterpResult<Value> {
        match info {
            ConstantInfo::Definition {
                name,
                level_params,
                value,
                ..
            }
            | ConstantInfo::Theorem {
                name,
                level_params,
                value,
                ..
            } => {
                let name_str = name.to_string();
                if name_str == "Array.foldlM.loop" {
                    return Ok(Value::Closure {
                        func: FuncRef::ArrayFoldlMLoop,
                        captured: vec![],
                        remaining_arity: 11,
                    });
                }
                if name_str == "StateRefT'.modifyGet" {
                    return Ok(Value::Closure {
                        func: FuncRef::StateRefModifyGet,
                        captured: vec![],
                        remaining_arity: 7,
                    });
                }
                let body = if !level_params.is_empty() && !levels.is_empty() {
                    value.instantiate_level_params(level_params, levels)
                } else {
                    value.clone()
                };
                let result = self.eval(&body, &LocalEnv::new())?;
                Ok(result)
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

            ConstantInfo::Inductive { .. } => Ok(Value::Erased),

            ConstantInfo::Opaque {
                name,
                level_params,
                value,
                ..
            } => {
                if self.trace_consts {
                    eprintln!(
                        "[trace] eval opaque {} (total_steps={})",
                        name, self.total_steps
                    );
                }
                let name_str = name.to_string();

                // BaseIO.asTask: run IO action synchronously, wrap result as Task
                if name_str == "BaseIO.asTask" {
                    return Ok(Value::Closure {
                        func: FuncRef::RunAction {
                            name: name.clone(),
                            arity: 4, // {α}, action, priority, world
                            action_idx: 1,
                            world_idx: 3,
                        },
                        captured: vec![],
                        remaining_arity: 4,
                    });
                }

                if name_str == "BaseIO.chainTask" {
                    return Ok(Value::Closure {
                        func: FuncRef::ChainTask,
                        captured: vec![],
                        remaining_arity: 6, // {α}, {β}, task, f, prio, world
                    });
                }

                // Check builtins for other opaques with @[extern] implementations
                if self.builtins.contains_key(name) {
                    return self.make_builtin_closure(name);
                }

                // Partial functions: opaque body is Inhabited.default (dummy).
                // The real implementation is in {name}._unsafe_rec (a def).
                let unsafe_rec_name = Name::mk_str(name.clone(), "_unsafe_rec");
                let unsafe_rec_ci = self.env.get(&unsafe_rec_name).ok().cloned();
                if let Some(ref ci) = unsafe_rec_ci {
                    if ci.is_definition() {
                        if self.trace_consts {
                            eprintln!("[trace] opaque {} -> using _unsafe_rec def", name);
                        }
                        return self.eval_const_info(ci, levels);
                    }
                }

                let body = if !level_params.is_empty() && !levels.is_empty() {
                    value.instantiate_level_params(level_params, levels)
                } else {
                    value.clone()
                };
                self.eval(&body, &LocalEnv::new())
            }

            ConstantInfo::Axiom { name, .. } => {
                let name_str = name.to_string();
                if name_str == "BaseIO.toEIO" {
                    // BaseIO.toEIO : ε → α → (BaseIO α) → ε → EStateM.Result ε α
                    // arity=4: [ε, α, act, world] → apply(args[2], args[3])
                    return Ok(Value::Closure {
                        func: FuncRef::ForwardApply {
                            name: name.clone(),
                            arity: 4,
                            func_idx: 2,
                            arg_idx: 3,
                        },
                        captured: vec![],
                        remaining_arity: 4,
                    });
                }
                if name_str == "Lean.Loop.forIn" {
                    return Ok(Value::Closure {
                        func: FuncRef::LoopForIn,
                        captured: vec![],
                        remaining_arity: 6,
                    });
                }
                if name_str == "StateRefT'.get" {
                    return Ok(Value::Closure {
                        func: FuncRef::StateRefGet,
                        captured: vec![],
                        remaining_arity: self.compute_arity_from_type(name),
                    });
                }
                if name_str == "StateRefT'.set" {
                    return Ok(Value::Closure {
                        func: FuncRef::StateRefSet,
                        captured: vec![],
                        remaining_arity: self.compute_arity_from_type(name),
                    });
                }
                if name_str == "StateRefT'.modifyGet" {
                    return Ok(Value::Closure {
                        func: FuncRef::StateRefModifyGet,
                        captured: vec![],
                        remaining_arity: self.compute_arity_from_type(name),
                    });
                }
                if name_str == "StateRefT'.run" {
                    return Ok(Value::Closure {
                        func: FuncRef::StateRefRun,
                        captured: vec![],
                        remaining_arity: self.compute_arity_from_type(name),
                    });
                }
                if name_str == "StateRefT'.lift" {
                    return Ok(Value::Closure {
                        func: FuncRef::StateRefLift,
                        captured: vec![],
                        remaining_arity: self.compute_arity_from_type(name),
                    });
                }
                if name_str == "EIO.toIO'" {
                    return Ok(Value::Closure {
                        func: FuncRef::EioToIoPrime,
                        captured: vec![],
                        remaining_arity: self.compute_arity_from_type(name),
                    });
                }
                if name_str == "EIO.toBaseIO" {
                    return Ok(Value::Closure {
                        func: FuncRef::EioToBaseIO,
                        captured: vec![],
                        remaining_arity: 4,
                    });
                }
                if name_str == "IO.FS.withIsolatedStreams" {
                    return Ok(Value::Closure {
                        func: FuncRef::WithIsolatedStreams,
                        captured: vec![],
                        remaining_arity: 8,
                    });
                }
                if self.builtins.contains_key(name) {
                    return self.make_builtin_closure(name);
                }

                if name_str == "EIO.catchExceptions" {
                    return Ok(Value::Closure {
                        func: FuncRef::EioCatchExceptions,
                        captured: vec![],
                        remaining_arity: 4,
                    });
                }

                // Partial functions compiled from `partial def`:
                // The axiom is a declaration; the real impl is {name}._unsafe_rec.
                let unsafe_rec_name = Name::mk_str(name.clone(), "_unsafe_rec");
                if let Ok(ci) = self.env.get(&unsafe_rec_name) {
                    if ci.is_definition() {
                        if self.trace_consts {
                            eprintln!("[trace] axiom {} -> using _unsafe_rec def", name);
                        }
                        return self.eval_const_info(&ci.clone(), levels);
                    }
                }

                if self.trace_consts {
                    eprintln!(
                        "[trace] ERASED axiom {} (total_steps={})",
                        name, self.total_steps
                    );
                }
                Ok(Value::Erased)
            }
            ConstantInfo::Quot { .. } => Ok(Value::Erased),
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

            if arity == 0 {
                1
            } else {
                arity
            }
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
            "ST", "EST", "EIO", "BaseIO", "IO", "ST'", "EST'", "StateM", "ReaderT", "StateT",
            "ExceptT", "EStateM",
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
            ConstantInfo::Definition {
                level_params,
                value,
                ..
            } => (level_params, value),
            _ => return None,
        };

        // Instantiate universe parameters
        let mut body = if !level_params.is_empty() && !levels.is_empty() {
            value.instantiate_level_params(level_params, levels)
        } else {
            value.clone()
        };

        // Apply arguments by stripping lambdas or constructing applications
        for arg in &args {
            match body.kind() {
                ExprKind::Lam(_, _, lam_body, _) => {
                    body = lam_body.instantiate(std::slice::from_ref(arg));
                }
                _ => {
                    // Body is a partial application (e.g., BaseIO = EIO Empty).
                    // Apply remaining args as App nodes instead of failing.
                    body = Expr::app(body, arg.clone());
                }
            }
        }

        Some(body)
    }

    /// Apply a function value to an argument.
    pub(crate) fn apply(&mut self, func: Value, arg: Value) -> InterpResult<Value> {
        stacker::maybe_grow(32 * 1024, 2 * 1024 * 1024, || {
            self.total_steps += 1;
            if self.max_steps > 0 && self.total_steps > self.max_steps {
                return Err(InterpError::StepLimitExceeded(self.total_steps));
            }
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
                Value::Erased => Ok(Value::Erased),
                Value::KernelExpr(e) => Ok(Value::KernelExpr(e)),
                // IO result Ctor applied as function — already computed, return as-is
                Value::Ctor { ref name, .. } if name.to_string().contains("EStateM.Result") => {
                    Ok(func)
                }
                _ => Err(InterpError::TypeError(format!(
                    "cannot apply non-function value: {:?} to arg: {:?}",
                    func, arg
                ))),
            }
        })
    }

    /// Execute a fully-applied function.
    fn apply_fully(&mut self, fref: FuncRef, args: Vec<Value>) -> InterpResult<Value> {
        match fref {
            FuncRef::Lambda(body, env) => {
                let arg = args.into_iter().last().unwrap();
                let new_env = env.push(arg);
                self.eval(&body, &new_env)
            }
            FuncRef::Definition(ref defname, ref levels) => {
                let val = self.eval_const(defname, levels)?;
                let mut result = val;
                for arg in args.into_iter() {
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
                match builtin_fn(&args) {
                    Ok(v) => Ok(v),
                    Err(InterpError::BuiltinError(ref _msg))
                        if args.iter().any(|v| matches!(v, Value::Erased)) =>
                    {
                        Ok(Value::Erased)
                    }
                    Err(e) => Err(e),
                }
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
                if name == Name::from_str_parts("Array.mk") {
                    if let Some(list_val) = fields.first() {
                        let mut elems = Vec::new();
                        let mut cur = list_val;
                        loop {
                            match cur {
                                Value::Ctor {
                                    tag: 1, fields: fs, ..
                                } if fs.len() >= 2 => {
                                    elems.push(fs[0].clone());
                                    cur = &fs[1];
                                }
                                _ => break,
                            }
                        }
                        return Ok(Value::Array(Arc::new(elems)));
                    }
                }
                Ok(Value::Ctor { tag, name, fields })
            }
            FuncRef::RecursorFn(name, levels) => iota::apply_recursor(self, &name, &levels, args),
            FuncRef::ForwardApply {
                name: _name,
                func_idx,
                arg_idx,
                ..
            } => {
                let func_val = args[func_idx as usize].clone();
                let arg_val = args[arg_idx as usize].clone();
                // If the forwarded function is Erased (type-erased IO action),
                // return io_ok(Erased, world) instead of apply(Erased, world) → Erased
                if matches!(&func_val, Value::Erased) {
                    return Ok(io_ok(Value::Erased, arg_val));
                }
                let result = self.apply(func_val, arg_val)?;
                Ok(result)
            }
            FuncRef::RunAction {
                name,
                action_idx,
                world_idx,
                ..
            } => {
                let action = args[action_idx as usize].clone();
                let world = args[world_idx as usize].clone();
                if self.trace_consts {
                    eprintln!(
                        "[RunAction] {} step={}: executing IO action synchronously",
                        name, self.total_steps
                    );
                }
                let result = self.apply(action, world.clone())?;
                match result {
                    Value::Ctor { tag: 0, fields, .. } if fields.len() >= 2 => {
                        let val = fields[0].clone();
                        let new_world = fields[1].clone();
                        let task = Value::Ref(std::sync::Arc::new(std::cell::RefCell::new(val)));
                        Ok(Value::Ctor {
                            tag: 0,
                            name: Name::from_str_parts("EStateM.Result.ok"),
                            fields: vec![task, new_world],
                        })
                    }
                    Value::Erased => Ok(io_ok(Value::Erased, world)),
                    other => Ok(other),
                }
            }
            FuncRef::ChainTask => {
                // args: [α, β, task, f, prio, world]
                let task_ref = &args[2];
                let f = args[3].clone();
                let world = args[5].clone();

                let task_val = match task_ref {
                    Value::Ref(r) => r.borrow().clone(),
                    Value::Erased => Value::Erased,
                    _ => Value::Erased,
                };

                let io_action = self.apply(f, task_val)?;
                let result = self.apply(io_action, world.clone())?;

                match result {
                    Value::Ctor { tag: 0, fields, .. } if fields.len() >= 2 => {
                        let val = fields[0].clone();
                        let new_world = fields[1].clone();
                        let new_task =
                            Value::Ref(std::sync::Arc::new(std::cell::RefCell::new(val)));
                        Ok(io_ok(new_task, new_world))
                    }
                    Value::Erased => Ok(io_ok(Value::Erased, world)),
                    other => Ok(other),
                }
            }
            FuncRef::LoopForIn => {
                // Lean.Loop.forIn args: [β(erased), m(erased), Monad_m, Loop.mk, init, f]
                let monad = &args[2];
                let init = args[4].clone();
                let f = args[5].clone();

                let bind_fn = Self::extract_bind_from_monad(monad);
                let pure_fn = Self::extract_pure_from_monad(monad);

                // f () init → action : m (ForInStep β)
                let f_unit = self.apply(f.clone(), Value::unit())?;
                let action = self.apply(f_unit, init)?;

                let cont = Value::Closure {
                    func: FuncRef::LoopContinuation {
                        f: Box::new(f),
                        bind_fn: Box::new(bind_fn.clone()),
                        pure_fn: Box::new(pure_fn),
                    },
                    captured: vec![],
                    remaining_arity: 1,
                };

                // bind {ForInStep β} {β} action cont
                let b1 = self.apply(bind_fn, Value::Erased)?;
                let b2 = self.apply(b1, Value::Erased)?;
                let b3 = self.apply(b2, action)?;
                self.apply(b3, cont)
            }
            FuncRef::LoopContinuation {
                f,
                bind_fn,
                pure_fn,
            } => {
                let step_val = args[0].clone();
                match &step_val {
                    Value::Ctor { tag: 0, fields, .. } => {
                        // ForInStep.done(b) → pure {β} b
                        let b = fields.first().cloned().unwrap_or(Value::Erased);
                        let p1 = self.apply(*pure_fn, Value::Erased)?;
                        self.apply(p1, b)
                    }
                    Value::Ctor { tag: 1, fields, .. } => {
                        // ForInStep.yield(b) → bind(f () b)(new_cont)
                        let b = fields.first().cloned().unwrap_or(Value::Erased);
                        let f_unit = self.apply(*f.clone(), Value::unit())?;
                        let action = self.apply(f_unit, b)?;

                        let cont = Value::Closure {
                            func: FuncRef::LoopContinuation {
                                f: f.clone(),
                                bind_fn: bind_fn.clone(),
                                pure_fn: pure_fn.clone(),
                            },
                            captured: vec![],
                            remaining_arity: 1,
                        };

                        let b1 = self.apply(*bind_fn, Value::Erased)?;
                        let b2 = self.apply(b1, Value::Erased)?;
                        let b3 = self.apply(b2, action)?;
                        self.apply(b3, cont)
                    }
                    Value::Erased => Ok(Value::Erased),
                    _ => Ok(Value::Erased),
                }
            }
            FuncRef::ArrayFoldlMLoop => {
                // args: [m, β, α, Monad, f, arr, stop, H, i, j, b]
                let monad = &args[3];
                let f = args[4].clone();
                let arr = match &args[5] {
                    Value::Array(a) => a.clone(),
                    _ => return Ok(Value::Erased),
                };
                let stop = {
                    let s = match &args[6] {
                        Value::Nat(n) => {
                            use num_traits::ToPrimitive;
                            n.to_u64().unwrap_or(0)
                        }
                        _ => 0,
                    };
                    s.min(arr.len() as u64)
                };
                let i = match &args[8] {
                    Value::Nat(n) => {
                        use num_traits::ToPrimitive;
                        n.to_u64().unwrap_or(0)
                    }
                    _ => 0,
                };
                let b = args[10].clone();

                let bind_fn = Self::extract_bind_from_monad(monad);
                let pure_fn = Self::extract_pure_from_monad(monad);

                if i >= stop {
                    let p1 = self.apply(pure_fn, Value::Erased)?;
                    self.apply(p1, b)
                } else {
                    let elem = arr.get(i as usize).cloned().unwrap_or(Value::Erased);
                    let f1 = self.apply(f.clone(), b)?;
                    let action = self.apply(f1, elem)?;

                    let cont = Value::Closure {
                        func: FuncRef::ArrayFoldlMCont {
                            f: Box::new(f),
                            arr,
                            stop,
                            current_idx: i + 1,
                            bind_fn: Box::new(bind_fn.clone()),
                            pure_fn: Box::new(pure_fn),
                        },
                        captured: vec![],
                        remaining_arity: 1,
                    };

                    let b1 = self.apply(bind_fn, Value::Erased)?;
                    let b2 = self.apply(b1, Value::Erased)?;
                    let b3 = self.apply(b2, action)?;
                    self.apply(b3, cont)
                }
            }
            FuncRef::ArrayFoldlMCont {
                f,
                arr,
                stop,
                current_idx,
                bind_fn,
                pure_fn,
            } => {
                let b = args[0].clone();

                if current_idx >= stop {
                    let p1 = self.apply(*pure_fn, Value::Erased)?;
                    self.apply(p1, b)
                } else {
                    let elem = arr
                        .get(current_idx as usize)
                        .cloned()
                        .unwrap_or(Value::Erased);
                    let f1 = self.apply(*f.clone(), b)?;
                    let action = self.apply(f1, elem)?;

                    let cont = Value::Closure {
                        func: FuncRef::ArrayFoldlMCont {
                            f,
                            arr,
                            stop,
                            current_idx: current_idx + 1,
                            bind_fn: bind_fn.clone(),
                            pure_fn: pure_fn.clone(),
                        },
                        captured: vec![],
                        remaining_arity: 1,
                    };

                    let b1 = self.apply(*bind_fn, Value::Erased)?;
                    let b2 = self.apply(b1, Value::Erased)?;
                    let b3 = self.apply(b2, action)?;
                    self.apply(b3, cont)
                }
            }
            FuncRef::StateRefGet => {
                let ref_val = args.iter().find(|v| matches!(v, Value::Ref(_)));
                if let Some(Value::Ref(r)) = ref_val {
                    let val = r.borrow().clone();
                    let world = args.last().cloned().unwrap_or(Value::Erased);
                    Ok(io_ok(val, world))
                } else {
                    Ok(Value::Erased)
                }
            }
            FuncRef::StateRefSet => {
                let ref_idx = args.iter().position(|v| matches!(v, Value::Ref(_)));
                if let Some(ri) = ref_idx {
                    let new_val = args[4..ri]
                        .iter()
                        .rfind(|v| !matches!(v, Value::Erased))
                        .cloned()
                        .unwrap_or(Value::Erased);
                    if let Value::Ref(r) = &args[ri] {
                        *r.borrow_mut() = new_val;
                    }
                    let world = args.last().cloned().unwrap_or(Value::Erased);
                    Ok(io_ok(Value::unit(), world))
                } else {
                    Ok(Value::Erased)
                }
            }
            FuncRef::StateRefModifyGet => {
                if args.len() < 7 {
                    return Ok(Value::Erased);
                }
                let Some(ri) = args.iter().position(|v| matches!(v, Value::Ref(_))) else {
                    return Ok(Value::Erased);
                };
                let Value::Ref(r) = &args[ri] else {
                    return Ok(Value::Erased);
                };

                let f = args[..ri]
                    .iter()
                    .rev()
                    .find(|v| matches!(v, Value::Closure { .. }))
                    .cloned()
                    .or_else(|| {
                        args[..ri]
                            .iter()
                            .rev()
                            .find(|v| !matches!(v, Value::Erased))
                            .cloned()
                    })
                    .unwrap_or(Value::Erased);

                let curr_state = r.borrow().clone();
                let pair_val = self.apply(f, curr_state)?;

                match pair_val {
                    Value::Ctor { fields, .. } if fields.len() >= 2 => {
                        let a = fields[0].clone();
                        let new_state = fields[1].clone();
                        *r.borrow_mut() = new_state;
                        let world = args.last().cloned().unwrap_or(Value::Erased);
                        Ok(io_ok(a, world))
                    }
                    Value::Erased => {
                        let world = args.last().cloned().unwrap_or(Value::Erased);
                        Ok(io_ok(Value::Erased, world))
                    }
                    _ => Ok(Value::Erased),
                }
            }
            FuncRef::StateRefRun => {
                if args.len() < 7 {
                    return Ok(Value::Erased);
                }
                let world = args.last().cloned().unwrap_or(Value::Erased);

                let state_idx_opt = args[..args.len().saturating_sub(1)]
                    .iter()
                    .rev()
                    .rposition(|v| !matches!(v, Value::Erased));
                let Some(state_idx) = state_idx_opt else {
                    return Ok(Value::Erased);
                };
                let s = args[state_idx].clone();
                let x = args[..state_idx]
                    .iter()
                    .rev()
                    .find(|v| matches!(v, Value::Closure { .. }))
                    .cloned()
                    .unwrap_or(Value::Erased);

                let state_ref = Value::Ref(std::sync::Arc::new(std::cell::RefCell::new(s)));
                let io_action = self.apply(x, state_ref.clone())?;
                let io_res = self.apply(io_action, world.clone())?;

                match io_res {
                    Value::Ctor { tag: 0, fields, .. } if fields.len() >= 2 => {
                        let a = fields[0].clone();
                        let new_world = fields[1].clone();
                        let final_s = match state_ref {
                            Value::Ref(r) => r.borrow().clone(),
                            _ => Value::Erased,
                        };
                        let pair = Value::Ctor {
                            tag: 0,
                            name: Name::from_str_parts("Prod.mk"),
                            fields: vec![a, final_s],
                        };
                        Ok(io_ok(pair, new_world))
                    }
                    Value::Ctor { tag: 1, .. } => Ok(io_res),
                    Value::Erased => Ok(io_ok(Value::Erased, world)),
                    other => Ok(other),
                }
            }
            FuncRef::StateRefLift => {
                if args.len() < 5 {
                    return Ok(Value::Erased);
                }
                let x = args
                    .iter()
                    .rev()
                    .find(|v| matches!(v, Value::Closure { .. }))
                    .cloned()
                    .or_else(|| {
                        args.iter()
                            .rev()
                            .find(|v| !matches!(v, Value::Erased))
                            .cloned()
                    })
                    .unwrap_or(Value::Erased);
                let some_ref = args
                    .iter()
                    .find(|v| matches!(v, Value::Ref(_)))
                    .cloned()
                    .unwrap_or(Value::Erased);
                Ok(Value::Closure {
                    func: FuncRef::IgnoreArgThenReturn { val: Box::new(x) },
                    captured: vec![some_ref],
                    remaining_arity: 1,
                })
            }
            FuncRef::IgnoreArgThenReturn { val } => {
                if args.is_empty() {
                    Ok(Value::Erased)
                } else {
                    Ok(*val)
                }
            }
            FuncRef::EioToIoPrime => {
                if args.len() < 4 {
                    return Ok(Value::Erased);
                }
                let world = args.last().cloned().unwrap_or(Value::Erased);
                let x = args[..args.len().saturating_sub(1)]
                    .iter()
                    .rev()
                    .find(|v| matches!(v, Value::Closure { .. }))
                    .cloned()
                    .or_else(|| {
                        args[..args.len().saturating_sub(1)]
                            .iter()
                            .rev()
                            .find(|v| !matches!(v, Value::Erased))
                            .cloned()
                    })
                    .unwrap_or(Value::Erased);
                self.apply(x, world)
            }
            FuncRef::EioToBaseIO => {
                let act = &args[2];
                let world = args[3].clone();
                let result = self.apply(act.clone(), world.clone())?;
                match &result {
                    Value::Ctor { tag: 0, fields, .. } => {
                        let val = fields.first().cloned().unwrap_or(Value::Erased);
                        let w = fields.get(1).cloned().unwrap_or(Value::Erased);
                        let except_ok = Value::Ctor {
                            name: Name::from_str_parts("Except.ok"),
                            tag: 1,
                            fields: vec![val],
                        };
                        Ok(io_ok(except_ok, w))
                    }
                    Value::Ctor { tag: 1, fields, .. } => {
                        let err = fields.first().cloned().unwrap_or(Value::Erased);
                        let w = fields.get(1).cloned().unwrap_or(Value::Erased);
                        let except_err = Value::Ctor {
                            name: Name::from_str_parts("Except.error"),
                            tag: 0,
                            fields: vec![err],
                        };
                        Ok(io_ok(except_err, w))
                    }
                    _ => Ok(io_ok(Value::Erased, world)),
                }
            }
            FuncRef::WithIsolatedStreams => {
                let action = &args[5];
                let world = args[7].clone();
                let result = self.apply(action.clone(), world.clone())?;
                match &result {
                    Value::Ctor { tag: 0, fields, .. } => {
                        let val = fields.first().cloned().unwrap_or(Value::Erased);
                        let w = fields.get(1).cloned().unwrap_or(Value::Erased);
                        let pair = Value::Ctor {
                            name: Name::from_str_parts("Prod.mk"),
                            tag: 0,
                            fields: vec![Value::String("".into()), val],
                        };
                        Ok(io_ok(pair, w))
                    }
                    _ => {
                        let pair = Value::Ctor {
                            name: Name::from_str_parts("Prod.mk"),
                            tag: 0,
                            fields: vec![Value::String("".into()), Value::Erased],
                        };
                        Ok(io_ok(pair, world))
                    }
                }
            }
            FuncRef::EioCatchExceptions => {
                // args: [α, act, defVal, world]
                let act = &args[1];
                let def_val = &args[2];
                let world = args[3].clone();
                let result = self.apply(act.clone(), world.clone())?;
                match &result {
                    Value::Ctor { tag: 0, fields, .. } => {
                        let val = fields.first().cloned().unwrap_or(Value::Erased);
                        let w = fields.get(1).cloned().unwrap_or(Value::Erased);
                        Ok(io_ok(val, w))
                    }
                    Value::Ctor { tag: 1, fields, .. } => {
                        let w = fields.get(1).cloned().unwrap_or(Value::Erased);
                        Ok(io_ok(def_val.clone(), w))
                    }
                    _ => Ok(io_ok(Value::Erased, world)),
                }
            }
        }
    }

    pub fn process_lean_input(
        &mut self,
        input: &str,
        file_name: &str,
    ) -> InterpResult<(Value, Value)> {
        let mk_input_ctx =
            self.eval_const(&Name::from_str_parts("Lean.Parser.mkInputContext"), &[])?;
        let input_ctx = self.apply(mk_input_ctx, Value::String(Arc::from(input)))?;
        let input_ctx = self.apply(input_ctx, Value::String(Arc::from(file_name)))?;
        let input_ctx = self.apply(input_ctx, Value::bool_(true))?;

        let empty_list = Value::Ctor {
            tag: 0,
            name: Name::from_str_parts("List.nil"),
            fields: vec![],
        };
        let opts_val = Value::Ctor {
            tag: 0,
            name: Name::from_str_parts("Lean.KVMap.mk"),
            fields: vec![empty_list],
        };
        let msg_empty = self.eval_const(&Name::from_str_parts("Lean.MessageLog.empty"), &[])?;

        let mk_command_state =
            self.eval_const(&Name::from_str_parts("Lean.Elab.Command.mkState"), &[])?;
        let command_state = self.apply(
            mk_command_state,
            Value::Environment(Arc::new(self.env.clone())),
        )?;
        let command_state = self.apply(command_state, msg_empty)?;
        let command_state = self.apply(command_state, opts_val)?;

        let mk_parser_state = self.eval_const(
            &Name::from_str_parts("Lean.Parser.ModuleParserState.mk"),
            &[],
        )?;
        let parser_state = self.apply(mk_parser_state, Value::nat_small(0))?;
        let parser_state = self.apply(parser_state, Value::bool_(false))?;

        let mk_frontend_ctx =
            self.eval_const(&Name::from_str_parts("Lean.Elab.Frontend.Context.mk"), &[])?;
        let frontend_ctx = self.apply(mk_frontend_ctx, input_ctx)?;

        let mk_frontend_state =
            self.eval_const(&Name::from_str_parts("Lean.Elab.Frontend.State.mk"), &[])?;
        let frontend_state = self.apply(mk_frontend_state, command_state)?;
        let frontend_state = self.apply(frontend_state, parser_state)?;
        let frontend_state = self.apply(frontend_state, Value::nat_small(0))?;
        let frontend_state = self.apply(frontend_state, Value::Array(Arc::new(vec![])))?;

        let process_cmds = self.eval_const(
            &Name::from_str_parts("Lean.Elab.Frontend.processCommands"),
            &[],
        )?;
        let state_action = self.apply(process_cmds, frontend_ctx)?;

        let state_ref = Value::Ref(std::sync::Arc::new(std::cell::RefCell::new(frontend_state)));
        let io_action = self.apply(state_action, state_ref.clone())?;
        let io_result = self.apply(io_action, Value::Erased)?;

        match io_result {
            Value::Ctor {
                tag: 0,
                ref name,
                ref fields,
            } if name == &Name::from_str_parts("EStateM.Result.ok") && fields.len() >= 2 => {}
            Value::Erased => {}
            Value::Ctor { tag: 1, fields, .. } => {
                let err = fields.first().cloned().unwrap_or(Value::Erased);
                return Err(InterpError::TypeError(format!(
                    "Lean.Elab.Frontend.processCommands returned IO error: {:?}",
                    err
                )));
            }
            _ => {
                return Err(InterpError::TypeError(format!(
                    "Lean.Elab.Frontend.processCommands returned non-ok result: {:?}",
                    io_result
                )));
            }
        }

        let final_frontend_state = match state_ref {
            Value::Ref(r) => r.borrow().clone(),
            _ => Value::Erased,
        };

        let command_state = match final_frontend_state {
            Value::Ctor { fields, .. } if !fields.is_empty() => fields[0].clone(),
            _ => {
                return Err(InterpError::TypeError(
                    "invalid Lean.Elab.Frontend.State shape".to_string(),
                ));
            }
        };

        match command_state {
            Value::Ctor { fields, .. } if fields.len() >= 2 => {
                Ok((fields[0].clone(), fields[1].clone()))
            }
            _ => Err(InterpError::TypeError(
                "invalid Lean.Elab.Command.State shape".to_string(),
            )),
        }
    }

    /// Evaluate a projection.
    fn eval_proj(&self, struct_name: &Name, idx: u64, val: Value) -> InterpResult<Value> {
        match val {
            Value::Ctor { fields, .. } => {
                if let Some(v) = fields.get(idx as usize) {
                    Ok(v.clone())
                } else {
                    eprintln!(
                        "[PROJ] step={} {}.{} out of range (has {} fields), returning Erased",
                        self.total_steps,
                        struct_name,
                        idx,
                        fields.len()
                    );
                    Ok(Value::Erased)
                }
            }
            Value::Erased => Ok(Value::Erased),
            // Char.0 projects the underlying Nat code point.
            // Subtype.0 projects the value; USize.size is stored as bare Nat(64).
            // In both cases, field 0 is the Nat itself, field 1 is erased proof.
            Value::Nat(_) if idx == 0 => Ok(val),
            Value::Nat(_) if idx == 1 => Ok(Value::Erased), // proof field
            Value::Environment(ref env_arc) => {
                let s = struct_name.to_string();
                if s.contains("Environment") && !s.contains("Header") {
                    if idx == 0 {
                        Ok(Value::Environment(env_arc.clone()))
                    } else {
                        Ok(Value::Erased)
                    }
                } else {
                    Ok(Value::Erased)
                }
            }
            Value::Array(ref items) => {
                if idx == 0 {
                    let mut list = Value::Ctor {
                        tag: 0,
                        name: Name::from_str_parts("List.nil"),
                        fields: vec![],
                    };
                    for item in items.iter().rev() {
                        list = Value::Ctor {
                            tag: 1,
                            name: Name::from_str_parts("List.cons"),
                            fields: vec![item.clone(), list],
                        };
                    }
                    Ok(list)
                } else {
                    Ok(Value::Erased)
                }
            }
            Value::String(ref s) => {
                if idx == 0 {
                    let (_name, fields) = iota::string_to_ctor(s);
                    Ok(fields.into_iter().next().unwrap_or(Value::Erased))
                } else {
                    Ok(Value::Erased)
                }
            }
            _ => Err(InterpError::TypeError(format!(
                "projection {}.{} on non-constructor value: {:?}",
                struct_name, idx, val
            ))),
        }
    }

    fn extract_bind_from_monad(monad: &Value) -> Value {
        // Monad.mk(toApplicative, toBind) → toBind = Bind.mk(bind_fn)
        match monad {
            Value::Ctor { fields, .. } if fields.len() >= 2 => match &fields[1] {
                Value::Ctor {
                    fields: bind_fields,
                    ..
                } if !bind_fields.is_empty() => bind_fields[0].clone(),
                other => other.clone(),
            },
            _ => Value::Erased,
        }
    }

    fn extract_pure_from_monad(monad: &Value) -> Value {
        // Monad.mk(toApplicative, toBind)
        //   toApplicative = Applicative.mk(toFunctor, toPure, ...)
        //     toPure = Pure.mk(pure_fn)
        match monad {
            Value::Ctor { fields, .. } if !fields.is_empty() => match &fields[0] {
                Value::Ctor {
                    fields: app_fields, ..
                } if app_fields.len() >= 2 => match &app_fields[1] {
                    Value::Ctor {
                        fields: pure_fields,
                        ..
                    } if !pure_fields.is_empty() => pure_fields[0].clone(),
                    other => other.clone(),
                },
                _ => Value::Erased,
            },
            _ => Value::Erased,
        }
    }

    fn eval_ite_short_circuit(
        &mut self,
        is_dite: bool,
        inst_expr: &Expr,
        then_expr: &Expr,
        else_expr: &Expr,
        env: &LocalEnv,
    ) -> InterpResult<Value> {
        let inst_val = self.eval(inst_expr, env)?;
        match &inst_val {
            Value::Ctor { tag: 1, fields, .. } => {
                if is_dite {
                    let h = fields.first().cloned().unwrap_or(Value::Erased);
                    let t = self.eval(then_expr, env)?;
                    self.apply(t, h)
                } else {
                    self.eval(then_expr, env)
                }
            }
            Value::Ctor { tag: 0, fields, .. } => {
                if is_dite {
                    let h = fields.first().cloned().unwrap_or(Value::Erased);
                    let e = self.eval(else_expr, env)?;
                    self.apply(e, h)
                } else {
                    self.eval(else_expr, env)
                }
            }
            _ => Ok(Value::Erased),
        }
    }
}
