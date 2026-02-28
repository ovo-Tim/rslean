use num_bigint::{BigInt, BigUint};
use num_traits::{One, Signed, Zero};
use rslean_name::Name;
use rustc_hash::FxHashMap;
use std::cell::RefCell;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use crate::error::{InterpError, InterpResult};
use crate::value::Value;

pub type BuiltinFn = fn(&[Value]) -> InterpResult<Value>;

/// Register all builtin functions.
pub fn register_builtins(map: &mut FxHashMap<Name, BuiltinFn>) {
    // Nat arithmetic
    reg(map, "Nat.add", nat_add);
    reg(map, "Nat.sub", nat_sub);
    reg(map, "Nat.mul", nat_mul);
    reg(map, "Nat.div", nat_div);
    reg(map, "Nat.mod", nat_mod);
    reg(map, "Nat.pow", nat_pow);
    reg(map, "Nat.gcd", nat_gcd);
    reg(map, "Nat.beq", nat_beq);
    reg(map, "Nat.ble", nat_ble);
    reg(map, "Nat.decEq", nat_dec_eq);
    reg(map, "Nat.decLe", nat_dec_le);
    reg(map, "Nat.decLt", nat_dec_lt);
    reg(map, "Nat.pred", nat_pred);
    reg(map, "Nat.land", nat_land);
    reg(map, "Nat.lor", nat_lor);
    reg(map, "Nat.xor", nat_xor);
    reg(map, "Nat.shiftLeft", nat_shift_left);
    reg(map, "Nat.shiftRight", nat_shift_right);

    // String operations
    reg(map, "String.decEq", string_dec_eq);
    reg(map, "String.append", string_append);
    reg(map, "String.length", string_length);
    reg(map, "String.mk", string_mk);
    reg(map, "String.push", string_push);
    reg(map, "String.utf8ByteSize", string_utf8_byte_size);

    // Bool
    reg(map, "Bool.decEq", bool_dec_eq);

    // Array operations
    reg(map, "Array.mkEmpty", array_mk_empty);
    reg(map, "Array.push", array_push);
    reg(map, "Array.size", array_size);
    reg(map, "Array.get!", array_get_bang);
    reg(map, "Array.set!", array_set_bang);

    // UInt32
    reg(map, "UInt32.ofNat", uint32_of_nat);
    reg(map, "UInt32.toNat", uint32_to_nat);
    reg(map, "UInt32.add", uint32_add);
    reg(map, "UInt32.sub", uint32_sub);
    reg(map, "UInt32.mul", uint32_mul);
    reg(map, "UInt32.div", uint32_div);
    reg(map, "UInt32.mod", uint32_mod);
    reg(map, "UInt32.decEq", uint32_dec_eq);
    reg(map, "UInt32.decLt", uint32_dec_lt);
    reg(map, "UInt32.decLe", uint32_dec_le);

    // USize
    reg(map, "USize.ofNat", usize_of_nat);
    reg(map, "USize.toNat", usize_to_nat);

    // UInt64
    reg(map, "UInt64.ofNat", uint64_of_nat);
    reg(map, "UInt64.toNat", uint64_to_nat);
    reg(map, "UInt64.add", uint64_add);
    reg(map, "UInt64.sub", uint64_sub);
    reg(map, "UInt64.mul", uint64_mul);
    reg(map, "UInt64.div", uint64_div);
    reg(map, "UInt64.mod", uint64_mod);
    reg(map, "UInt64.decEq", uint64_dec_eq);
    reg(map, "UInt64.decLt", uint64_dec_lt);
    reg(map, "UInt64.decLe", uint64_dec_le);

    // UInt8 / UInt16
    reg(map, "UInt8.ofNat", uint8_of_nat);
    reg(map, "UInt8.toNat", uint8_to_nat);
    reg(map, "UInt16.ofNat", uint16_of_nat);
    reg(map, "UInt16.toNat", uint16_to_nat);

    // Char
    reg(map, "Char.ofNat", char_of_nat);
    reg(map, "Char.toNat", char_to_nat);

    // ST/Ref — mutable state (monadic: last arg is world token, return ST.Out.mk)
    reg(map, "ST.Prim.mkRef", st_prim_mk_ref);
    reg(map, "ST.Prim.Ref.get", st_prim_ref_get);
    reg(map, "ST.Prim.Ref.set", st_prim_ref_set);
    reg(map, "ST.Prim.Ref.swap", st_prim_ref_swap);
    reg(map, "ST.Prim.Ref.modifyGet", st_prim_ref_modify_get);
    // Legacy names (some .olean files use these)
    reg(map, "ST.Ref.mk", st_prim_mk_ref);
    reg(map, "ST.Ref.get", st_prim_ref_get);
    reg(map, "ST.Ref.set", st_prim_ref_set);
    reg(map, "ST.Ref.modifyGet", st_prim_ref_modify_get);

    // IO builtins (monadic: last arg is world token, return EStateM.Result.ok)
    reg(map, "IO.println", io_println);
    reg(map, "IO.print", io_print);
    reg(map, "IO.eprintln", io_eprintln);

    // String (additional)
    reg(map, "String.data", string_data);
    reg(map, "String.intercalate", string_intercalate);
    reg(map, "String.isEmpty", string_is_empty);
    reg(map, "String.get", string_get);
    reg(map, "String.take", string_take);
    reg(map, "String.drop", string_drop);
    reg(map, "String.trimRight", string_trim_right);

    // Thunk
    reg(map, "Thunk.pure", thunk_pure);
    reg(map, "Thunk.get", thunk_get);

    // Platform builtins (IO actions returning values)
    reg(map, "System.Platform.getIsWindows", platform_get_is_windows);
    reg(map, "System.Platform.getIsOSX", platform_get_is_osx);
    reg(map, "System.Platform.getIsEmscripten", platform_get_is_emscripten);
    reg(map, "System.Platform.getNumBits", platform_get_num_bits);

    // IO timing/heartbeat stubs
    reg(map, "IO.monoMsNow", io_mono_ms_now);
    reg(map, "IO.monoNanosNow", io_mono_nanos_now);
    reg(map, "IO.getNumHeartbeats", io_get_num_heartbeats);
    reg(map, "IO.initializing", io_initializing);

    // HashMap builtins
    reg(map, "Lean.HashMap.mkEmpty", hashmap_mk_empty);
    reg(map, "Lean.HashMap.empty", hashmap_mk_empty);
    reg(map, "Lean.HashMap.insert", hashmap_insert);
    reg(map, "Lean.HashMap.find?", hashmap_find);
    reg(map, "Lean.HashMap.size", hashmap_size);
    reg(map, "Lean.HashMap.contains", hashmap_contains);

    // ByteArray builtins
    reg(map, "ByteArray.mkEmpty", bytearray_mk_empty);
    reg(map, "ByteArray.push", bytearray_push);
    reg(map, "ByteArray.size", bytearray_size);
    reg(map, "ByteArray.get!", bytearray_get_bang);

    // Additional Array builtins
    reg(map, "Array.fget", array_fget);
    reg(map, "Array.fset", array_fset);
    reg(map, "Array.pop", array_pop);
    reg(map, "Array.fswap", array_fswap);
    reg(map, "Array.swap", array_fswap);
    reg(map, "Array.uget", array_uget);

    // Int builtins
    reg(map, "Int.ofNat", int_of_nat);
    reg(map, "Int.negSucc", int_neg_succ);
    reg(map, "Int.add", int_add);
    reg(map, "Int.sub", int_sub);
    reg(map, "Int.mul", int_mul);
    reg(map, "Int.div", int_div);
    reg(map, "Int.mod", int_mod);
    reg(map, "Int.neg", int_neg);
    reg(map, "Int.decEq", int_dec_eq);
    reg(map, "Int.decLe", int_dec_le);
    reg(map, "Int.decLt", int_dec_lt);
    reg(map, "Int.decNonneg", int_dec_nonneg);
    reg(map, "Int.toNat", int_to_nat);

    // Name builtins
    reg(map, "Name.beq", name_beq);
    reg(map, "Name.hash", name_hash);
    reg(map, "Name.mkStr", name_mk_str);
    reg(map, "Name.mkNum", name_mk_num);

    // USize additional
    reg(map, "USize.add", usize_add);
    reg(map, "USize.sub", usize_sub);
    reg(map, "USize.mul", usize_mul);
    reg(map, "USize.div", usize_div);
    reg(map, "USize.mod", usize_mod);
    reg(map, "USize.decEq", usize_dec_eq);
    reg(map, "USize.decLt", usize_dec_lt);
    reg(map, "USize.decLe", usize_dec_le);

    // Float stubs
    reg(map, "Float.ofScientific", float_of_scientific);
    reg(map, "Float.toString", float_to_string);

    // Lean.Expr structural operations
    // These operate on Lean.Expr values as Value::Ctor or Value::KernelExpr
    reg(map, "Lean.Expr.eqv", lean_expr_eqv);
    reg(map, "Lean.Expr.lt", lean_expr_lt);
    reg(map, "Lean.Expr.hash", lean_expr_hash);
    reg(map, "Lean.Expr.bvar!", lean_expr_bvar_idx);
    reg(map, "Lean.Expr.fvarId!", lean_expr_fvar_id);
    reg(map, "Lean.Expr.mvarId!", lean_expr_mvar_id);
    reg(map, "Lean.Expr.isBVar", lean_expr_is_bvar);
    reg(map, "Lean.Expr.isFVar", lean_expr_is_fvar);
    reg(map, "Lean.Expr.isMVar", lean_expr_is_mvar);
    reg(map, "Lean.Expr.isSort", lean_expr_is_sort);
    reg(map, "Lean.Expr.isConst", lean_expr_is_const);
    reg(map, "Lean.Expr.isApp", lean_expr_is_app);
    reg(map, "Lean.Expr.isLambda", lean_expr_is_lambda);
    reg(map, "Lean.Expr.isForall", lean_expr_is_forall);
    reg(map, "Lean.Expr.isLet", lean_expr_is_let);
    reg(map, "Lean.Expr.isLit", lean_expr_is_lit);
    reg(map, "Lean.Expr.isMData", lean_expr_is_mdata);
    reg(map, "Lean.Expr.isProj", lean_expr_is_proj);
    reg(map, "Lean.Expr.hasLooseBVars", lean_expr_has_loose_bvars);
    reg(map, "Lean.Expr.looseBVarRange", lean_expr_loose_bvar_range);
    reg(map, "Lean.Expr.hasFVar", lean_expr_has_fvar);
    reg(map, "Lean.Expr.hasMVar", lean_expr_has_mvar);
    reg(map, "Lean.Expr.approxDepth", lean_expr_approx_depth);
    reg(map, "Lean.Expr.headBeta", lean_expr_head_beta);
    reg(map, "Lean.Expr.getAppNumArgs", lean_expr_get_app_num_args);
    // Expr constructor operations
    reg(map, "Lean.mkBVar", lean_mk_bvar);
    reg(map, "Lean.mkFVar", lean_mk_fvar);
    reg(map, "Lean.mkMVar", lean_mk_mvar);
    reg(map, "Lean.mkSort", lean_mk_sort);
    reg(map, "Lean.mkConst", lean_mk_const);
    reg(map, "Lean.mkApp", lean_mk_app);
    reg(map, "Lean.mkApp2", lean_mk_app2);
    reg(map, "Lean.mkApp3", lean_mk_app3);
    reg(map, "Lean.mkAppN", lean_mk_app_n);
    reg(map, "Lean.mkLambda", lean_mk_lambda);
    reg(map, "Lean.mkForall", lean_mk_forall);
    reg(map, "Lean.mkLet", lean_mk_let);
    reg(map, "Lean.mkLit", lean_mk_lit);
    reg(map, "Lean.mkMData", lean_mk_mdata);
    reg(map, "Lean.mkProj", lean_mk_proj);
    reg(map, "Lean.Expr.dbgToString", lean_expr_dbg_to_string);
    reg(map, "Lean.Expr.ctorIdx", lean_expr_ctor_idx);

    // Lean.Name structural operations
    reg(map, "Lean.Name.beq", lean_name_beq);
    reg(map, "Lean.Name.hash", lean_name_hash);
    reg(map, "Lean.Name.str", lean_name_str);
    reg(map, "Lean.Name.num", lean_name_num);
    reg(map, "Lean.Name.isAnonymous", lean_name_is_anonymous);
    reg(map, "Lean.Name.isStr", lean_name_is_str);
    reg(map, "Lean.Name.isNum", lean_name_is_num);
    reg(map, "Lean.Name.getString!", lean_name_get_string);
    reg(map, "Lean.Name.getNum!", lean_name_get_num);
    reg(map, "Lean.Name.append", lean_name_append);
    reg(map, "Lean.Name.toString", lean_name_to_string);
    reg(map, "Lean.Name.quickLt", lean_name_quick_lt);

    // Lean.Level structural operations
    reg(map, "Lean.Level.beq", lean_level_beq);
    reg(map, "Lean.Level.hash", lean_level_hash);
    reg(map, "Lean.Level.isZero", lean_level_is_zero);
    reg(map, "Lean.Level.isSucc", lean_level_is_succ);
    reg(map, "Lean.Level.isMax", lean_level_is_max);
    reg(map, "Lean.Level.isIMax", lean_level_is_imax);
    reg(map, "Lean.Level.isParam", lean_level_is_param);
    reg(map, "Lean.Level.isMVar", lean_level_is_mvar);
    reg(map, "Lean.Level.succ!", lean_level_succ_of);
    reg(map, "Lean.Level.max!", lean_level_max_of);
    reg(map, "Lean.Level.imax!", lean_level_imax_of);
    reg(map, "Lean.Level.param!", lean_level_param_of);
    reg(map, "Lean.Level.mvar!", lean_level_mvar_of);

    // Lean.Environment bridge (stub implementations)
    reg(map, "Lean.Environment.find?", lean_env_find);
    reg(map, "Lean.Environment.contains", lean_env_contains);
    reg(map, "Lean.Environment.isConstructor", lean_env_is_constructor);
    reg(map, "Lean.Environment.isInductive", lean_env_is_inductive);
    reg(map, "Lean.Environment.isRecursor", lean_env_is_recursor);

    // Lean.RBTree / PersistentHashMap stubs (needed by elaborator data structures)
    reg(map, "Lean.RBNode.depth", lean_rbnode_depth);
    reg(map, "Lean.PersistentHashMap.mkEmptyEntries", lean_persistent_hashmap_mk_empty);

    // Additional IO stubs
    reg(map, "IO.getStdout", io_get_stdout);
    reg(map, "IO.getStderr", io_get_stderr);
    reg(map, "IO.getStdin", io_get_stdin);
    reg(map, "IO.Handle.putStr", io_handle_put_str);
    reg(map, "IO.Handle.flush", io_handle_flush);
    reg(map, "IO.Handle.putStrLn", io_handle_put_str_ln);
    reg(map, "IO.getEnv", io_get_env);
    reg(map, "IO.isEOF", io_is_eof);
    reg(map, "IO.getLine", io_get_line);
    reg(map, "IO.Error.toString", io_error_to_string);
    reg(map, "IO.Error.userError", io_error_user_error);

    // String extra builtins
    reg(map, "String.toNat?", string_to_nat_opt);
    reg(map, "String.toInt?", string_to_int_opt);
    reg(map, "String.startsWith", string_starts_with);
    reg(map, "String.endsWith", string_ends_with);
    reg(map, "String.contains", string_contains_char);
    reg(map, "String.splitOn", string_split_on);
    reg(map, "String.replace", string_replace);
    reg(map, "String.trim", string_trim);
    reg(map, "String.trimLeft", string_trim_left);
    reg(map, "String.toList", string_to_list);

    // Option helpers used by various builtins
    reg(map, "Option.isSome", option_is_some);
    reg(map, "Option.isNone", option_is_none);
    reg(map, "Option.get!", option_get_bang);
}

fn reg(map: &mut FxHashMap<Name, BuiltinFn>, name: &str, f: BuiltinFn) {
    map.insert(Name::from_str_parts(name), f);
}

/// Call a builtin by name (for testing).
#[cfg(test)]
pub fn test_builtin_call(name: &str, args: &[Value]) -> InterpResult<Value> {
    let mut map = FxHashMap::default();
    register_builtins(&mut map);
    let f = map.get(&Name::from_str_parts(name))
        .ok_or_else(|| InterpError::UnknownConstant(Name::from_str_parts(name)))?;
    f(args)
}

// --------------- Nat builtins ---------------

fn extract_nat(args: &[Value], idx: usize) -> InterpResult<&BigUint> {
    args.get(idx)
        .and_then(|v| v.as_nat())
        .ok_or_else(|| InterpError::BuiltinError(format!("expected Nat at arg {}", idx)))
}

fn nat_add(args: &[Value]) -> InterpResult<Value> {
    let a = extract_nat(args, 0)?;
    let b = extract_nat(args, 1)?;
    Ok(Value::nat(a + b))
}

fn nat_sub(args: &[Value]) -> InterpResult<Value> {
    let a = extract_nat(args, 0)?;
    let b = extract_nat(args, 1)?;
    if a >= b {
        Ok(Value::nat(a - b))
    } else {
        Ok(Value::nat(BigUint::zero()))
    }
}

fn nat_mul(args: &[Value]) -> InterpResult<Value> {
    let a = extract_nat(args, 0)?;
    let b = extract_nat(args, 1)?;
    Ok(Value::nat(a * b))
}

fn nat_div(args: &[Value]) -> InterpResult<Value> {
    let a = extract_nat(args, 0)?;
    let b = extract_nat(args, 1)?;
    if b.is_zero() {
        Ok(Value::nat(BigUint::zero()))
    } else {
        Ok(Value::nat(a / b))
    }
}

fn nat_mod(args: &[Value]) -> InterpResult<Value> {
    let a = extract_nat(args, 0)?;
    let b = extract_nat(args, 1)?;
    if b.is_zero() {
        Ok(Value::nat(BigUint::zero()))
    } else {
        Ok(Value::nat(a % b))
    }
}

fn nat_pow(args: &[Value]) -> InterpResult<Value> {
    let base = extract_nat(args, 0)?;
    let exp = extract_nat(args, 1)?;
    // Limit exponent to prevent runaway computation
    let exp_u32: u32 = exp
        .try_into()
        .map_err(|_| InterpError::BuiltinError("Nat.pow: exponent too large".into()))?;
    Ok(Value::nat(num_traits::pow::Pow::pow(base, exp_u32)))
}

fn nat_gcd(args: &[Value]) -> InterpResult<Value> {
    use num_integer::Integer;
    let a = extract_nat(args, 0)?;
    let b = extract_nat(args, 1)?;
    Ok(Value::nat(a.gcd(b)))
}

fn nat_beq(args: &[Value]) -> InterpResult<Value> {
    let a = extract_nat(args, 0)?;
    let b = extract_nat(args, 1)?;
    Ok(Value::bool_(a == b))
}

fn nat_ble(args: &[Value]) -> InterpResult<Value> {
    let a = extract_nat(args, 0)?;
    let b = extract_nat(args, 1)?;
    Ok(Value::bool_(a <= b))
}

fn nat_dec_eq(args: &[Value]) -> InterpResult<Value> {
    // Nat.decEq : (a b : Nat) → Decidable (a = b)
    // Skip type args if present. The actual Nat args may be at different positions.
    let (a, b) = find_two_nats(args)?;
    Ok(make_decidable(a == b))
}

fn nat_dec_le(args: &[Value]) -> InterpResult<Value> {
    let (a, b) = find_two_nats(args)?;
    Ok(make_decidable(a <= b))
}

fn nat_dec_lt(args: &[Value]) -> InterpResult<Value> {
    let (a, b) = find_two_nats(args)?;
    Ok(make_decidable(a < b))
}

fn nat_pred(args: &[Value]) -> InterpResult<Value> {
    let a = extract_nat(args, 0)?;
    if a.is_zero() {
        Ok(Value::nat(BigUint::zero()))
    } else {
        Ok(Value::nat(a - BigUint::one()))
    }
}

fn nat_land(args: &[Value]) -> InterpResult<Value> {
    let a = extract_nat(args, 0)?;
    let b = extract_nat(args, 1)?;
    Ok(Value::nat(a & b))
}

fn nat_lor(args: &[Value]) -> InterpResult<Value> {
    let a = extract_nat(args, 0)?;
    let b = extract_nat(args, 1)?;
    Ok(Value::nat(a | b))
}

fn nat_xor(args: &[Value]) -> InterpResult<Value> {
    let a = extract_nat(args, 0)?;
    let b = extract_nat(args, 1)?;
    Ok(Value::nat(a ^ b))
}

fn nat_shift_left(args: &[Value]) -> InterpResult<Value> {
    let a = extract_nat(args, 0)?;
    let b = extract_nat(args, 1)?;
    let shift: u64 = b
        .try_into()
        .map_err(|_| InterpError::BuiltinError("shift amount too large".into()))?;
    Ok(Value::nat(a << shift))
}

fn nat_shift_right(args: &[Value]) -> InterpResult<Value> {
    let a = extract_nat(args, 0)?;
    let b = extract_nat(args, 1)?;
    let shift: u64 = b
        .try_into()
        .map_err(|_| InterpError::BuiltinError("shift amount too large".into()))?;
    Ok(Value::nat(a >> shift))
}

// --------------- String builtins ---------------

fn extract_str(args: &[Value], idx: usize) -> InterpResult<&str> {
    args.get(idx)
        .and_then(|v| v.as_str())
        .ok_or_else(|| InterpError::BuiltinError(format!("expected String at arg {}", idx)))
}

fn string_dec_eq(args: &[Value]) -> InterpResult<Value> {
    let (a, b) = find_two_strs(args)?;
    Ok(make_decidable(a == b))
}

fn string_append(args: &[Value]) -> InterpResult<Value> {
    let a = extract_str(args, 0)?;
    let b = extract_str(args, 1)?;
    let mut result = String::with_capacity(a.len() + b.len());
    result.push_str(a);
    result.push_str(b);
    Ok(Value::string(result.as_str()))
}

fn string_length(args: &[Value]) -> InterpResult<Value> {
    let s = extract_str(args, 0)?;
    Ok(Value::nat_small(s.chars().count() as u64))
}

fn string_mk(args: &[Value]) -> InterpResult<Value> {
    // String.mk : List Char → String
    // For now, return empty string as a stub
    let _ = args;
    Ok(Value::string(""))
}

fn string_push(args: &[Value]) -> InterpResult<Value> {
    let s = extract_str(args, 0)?;
    // Second arg should be a Char (represented as Nat/UInt32)
    let ch_val = args.get(1).ok_or_else(|| {
        InterpError::BuiltinError("String.push: missing char arg".into())
    })?;
    let ch = match ch_val {
        Value::Nat(n) => {
            let code: u32 = n.as_ref().try_into().unwrap_or(0u32);
            char::from_u32(code).unwrap_or('\u{FFFD}')
        }
        _ => '\u{FFFD}',
    };
    let mut result = s.to_string();
    result.push(ch);
    Ok(Value::string(result.as_str()))
}

fn string_utf8_byte_size(args: &[Value]) -> InterpResult<Value> {
    let s = extract_str(args, 0)?;
    Ok(Value::nat_small(s.len() as u64))
}

// --------------- Bool builtins ---------------

fn bool_dec_eq(args: &[Value]) -> InterpResult<Value> {
    let a = args
        .first()
        .and_then(|v| v.as_bool())
        .ok_or_else(|| InterpError::BuiltinError("expected Bool at arg 0".into()))?;
    let b = args
        .get(1)
        .and_then(|v| v.as_bool())
        .ok_or_else(|| InterpError::BuiltinError("expected Bool at arg 1".into()))?;
    Ok(make_decidable(a == b))
}

// --------------- Array builtins ---------------

fn array_mk_empty(args: &[Value]) -> InterpResult<Value> {
    // Array.mkEmpty : {α : Type} → Nat → Array α
    // Skip type arg and capacity
    let _ = args;
    Ok(Value::Array(Arc::new(Vec::new())))
}

fn array_push(args: &[Value]) -> InterpResult<Value> {
    // Array.push : {α : Type} → Array α → α → Array α
    // Find the array and value args
    let (arr, val) = find_array_and_val(args)?;
    let mut new_arr = (*arr).clone();
    new_arr.push(val.clone());
    Ok(Value::Array(Arc::new(new_arr)))
}

fn array_size(args: &[Value]) -> InterpResult<Value> {
    let arr = find_array(args)?;
    Ok(Value::nat_small(arr.len() as u64))
}

fn array_get_bang(args: &[Value]) -> InterpResult<Value> {
    // Array.get! : {α : Type} → [Inhabited α] → Array α → Nat → α
    let arr = find_array(args)?;
    let idx = find_last_nat(args)?;
    let idx_usize: usize = idx
        .try_into()
        .map_err(|_| InterpError::BuiltinError("Array index too large".into()))?;
    arr.get(idx_usize)
        .cloned()
        .ok_or_else(|| InterpError::BuiltinError(format!("Array index {} out of bounds", idx)))
}

fn array_set_bang(args: &[Value]) -> InterpResult<Value> {
    // Array.set! : {α : Type} → Array α → Nat → α → Array α
    let arr = find_array(args)?;
    let mut new_arr = (*arr).clone();
    // Find index and value
    let nats: Vec<&BigUint> = args.iter().filter_map(|v| v.as_nat()).collect();
    if let Some(idx) = nats.first() {
        let idx_usize: usize = (*idx)
            .try_into()
            .map_err(|_| InterpError::BuiltinError("Array index too large".into()))?;
        // Find the last non-Nat, non-Array, non-Erased arg as the value
        if let Some(val) = args.iter().rev().find(|v| {
            !matches!(v, Value::Nat(_) | Value::Array(_) | Value::Erased)
        }) {
            if idx_usize < new_arr.len() {
                new_arr[idx_usize] = val.clone();
            }
        }
    }
    Ok(Value::Array(Arc::new(new_arr)))
}

// --------------- UInt32 builtins ---------------

const UINT32_MOD: u64 = 1u64 << 32;

fn uint32_of_nat(args: &[Value]) -> InterpResult<Value> {
    let n = find_last_nat_val(args)?;
    let n_u64: u64 = n.try_into().unwrap_or(u64::MAX);
    Ok(Value::nat_small(n_u64 % UINT32_MOD))
}

fn uint32_to_nat(args: &[Value]) -> InterpResult<Value> {
    let n = find_last_nat_val(args)?;
    Ok(Value::nat(n.clone()))
}

fn uint32_add(args: &[Value]) -> InterpResult<Value> {
    let (a, b) = find_two_nats(args)?;
    let a_u64: u64 = a.try_into().unwrap_or(0);
    let b_u64: u64 = b.try_into().unwrap_or(0);
    Ok(Value::nat_small((a_u64.wrapping_add(b_u64)) % UINT32_MOD))
}

fn uint32_sub(args: &[Value]) -> InterpResult<Value> {
    let (a, b) = find_two_nats(args)?;
    let a_u64: u64 = a.try_into().unwrap_or(0);
    let b_u64: u64 = b.try_into().unwrap_or(0);
    Ok(Value::nat_small((a_u64.wrapping_sub(b_u64)) % UINT32_MOD))
}

fn uint32_mul(args: &[Value]) -> InterpResult<Value> {
    let (a, b) = find_two_nats(args)?;
    let a_u64: u64 = a.try_into().unwrap_or(0);
    let b_u64: u64 = b.try_into().unwrap_or(0);
    Ok(Value::nat_small((a_u64.wrapping_mul(b_u64)) % UINT32_MOD))
}

fn uint32_div(args: &[Value]) -> InterpResult<Value> {
    let (a, b) = find_two_nats(args)?;
    let a_u64: u64 = a.try_into().unwrap_or(0);
    let b_u64: u64 = b.try_into().unwrap_or(0);
    if b_u64 == 0 {
        Ok(Value::nat_small(0))
    } else {
        Ok(Value::nat_small(a_u64 / b_u64))
    }
}

fn uint32_mod(args: &[Value]) -> InterpResult<Value> {
    let (a, b) = find_two_nats(args)?;
    let a_u64: u64 = a.try_into().unwrap_or(0);
    let b_u64: u64 = b.try_into().unwrap_or(0);
    if b_u64 == 0 {
        Ok(Value::nat_small(0))
    } else {
        Ok(Value::nat_small(a_u64 % b_u64))
    }
}

fn uint32_dec_eq(args: &[Value]) -> InterpResult<Value> {
    let (a, b) = find_two_nats(args)?;
    Ok(make_decidable(a == b))
}

fn uint32_dec_lt(args: &[Value]) -> InterpResult<Value> {
    let (a, b) = find_two_nats(args)?;
    Ok(make_decidable(a < b))
}

fn uint32_dec_le(args: &[Value]) -> InterpResult<Value> {
    let (a, b) = find_two_nats(args)?;
    Ok(make_decidable(a <= b))
}

// --------------- USize builtins ---------------

fn usize_of_nat(args: &[Value]) -> InterpResult<Value> {
    let n = find_last_nat_val(args)?;
    let n_u64: u64 = n.try_into().unwrap_or(u64::MAX);
    // USize is platform-dependent, but we use 64-bit
    Ok(Value::nat_small(n_u64))
}

fn usize_to_nat(args: &[Value]) -> InterpResult<Value> {
    let n = find_last_nat_val(args)?;
    Ok(Value::nat(n.clone()))
}

// --------------- Helpers ---------------

/// Find two Nat arguments, skipping Erased values.
fn find_two_nats(args: &[Value]) -> InterpResult<(&BigUint, &BigUint)> {
    let nats: Vec<&BigUint> = args.iter().filter_map(|v| v.as_nat()).collect();
    if nats.len() >= 2 {
        Ok((nats[0], nats[1]))
    } else {
        Err(InterpError::BuiltinError(format!(
            "expected 2 Nat args, found {}",
            nats.len()
        )))
    }
}

fn find_two_strs(args: &[Value]) -> InterpResult<(&str, &str)> {
    let strs: Vec<&str> = args.iter().filter_map(|v| v.as_str()).collect();
    if strs.len() >= 2 {
        Ok((strs[0], strs[1]))
    } else {
        Err(InterpError::BuiltinError(format!(
            "expected 2 String args, found {}",
            strs.len()
        )))
    }
}

fn find_last_nat(args: &[Value]) -> InterpResult<&BigUint> {
    args.iter()
        .rev()
        .find_map(|v| v.as_nat())
        .ok_or_else(|| InterpError::BuiltinError("expected Nat arg".into()))
}

fn find_last_nat_val(args: &[Value]) -> InterpResult<&BigUint> {
    find_last_nat(args)
}

fn find_array(args: &[Value]) -> InterpResult<&Vec<Value>> {
    args.iter()
        .find_map(|v| match v {
            Value::Array(a) => Some(a.as_ref()),
            _ => None,
        })
        .ok_or_else(|| InterpError::BuiltinError("expected Array arg".into()))
}

fn find_array_and_val(args: &[Value]) -> InterpResult<(&Vec<Value>, &Value)> {
    let arr = find_array(args)?;
    // The value to push is the last non-Erased, non-Array arg
    let val = args
        .iter()
        .rev()
        .find(|v| !matches!(v, Value::Array(_) | Value::Erased))
        .ok_or_else(|| InterpError::BuiltinError("expected value arg for Array.push".into()))?;
    Ok((arr, val))
}

/// Construct a `Decidable` value.
fn make_decidable(b: bool) -> Value {
    if b {
        // Decidable.isTrue : proof → Decidable p
        // tag 1 = isTrue
        Value::Ctor {
            tag: 1,
            name: Name::from_str_parts("Decidable.isTrue"),
            fields: vec![Value::Erased], // proof is erased
        }
    } else {
        // Decidable.isFalse : ¬p → Decidable p
        // tag 0 = isFalse
        Value::Ctor {
            tag: 0,
            name: Name::from_str_parts("Decidable.isFalse"),
            fields: vec![Value::Erased], // proof is erased
        }
    }
}

// --------------- UInt64 builtins ---------------

const UINT64_MOD: u128 = 1u128 << 64;

fn uint64_of_nat(args: &[Value]) -> InterpResult<Value> {
    let n = find_last_nat_val(args)?;
    let n_u64: u64 = n.try_into().unwrap_or(u64::MAX);
    Ok(Value::nat_small(n_u64))
}

fn uint64_to_nat(args: &[Value]) -> InterpResult<Value> {
    let n = find_last_nat_val(args)?;
    Ok(Value::nat(n.clone()))
}

fn uint64_add(args: &[Value]) -> InterpResult<Value> {
    let (a, b) = find_two_nats(args)?;
    let a_u64: u64 = a.try_into().unwrap_or(0);
    let b_u64: u64 = b.try_into().unwrap_or(0);
    let result = ((a_u64 as u128 + b_u64 as u128) % UINT64_MOD) as u64;
    Ok(Value::nat_small(result))
}

fn uint64_sub(args: &[Value]) -> InterpResult<Value> {
    let (a, b) = find_two_nats(args)?;
    let a_u64: u64 = a.try_into().unwrap_or(0);
    let b_u64: u64 = b.try_into().unwrap_or(0);
    Ok(Value::nat_small(a_u64.wrapping_sub(b_u64)))
}

fn uint64_mul(args: &[Value]) -> InterpResult<Value> {
    let (a, b) = find_two_nats(args)?;
    let a_u64: u64 = a.try_into().unwrap_or(0);
    let b_u64: u64 = b.try_into().unwrap_or(0);
    let result = ((a_u64 as u128 * b_u64 as u128) % UINT64_MOD) as u64;
    Ok(Value::nat_small(result))
}

fn uint64_div(args: &[Value]) -> InterpResult<Value> {
    let (a, b) = find_two_nats(args)?;
    let a_u64: u64 = a.try_into().unwrap_or(0);
    let b_u64: u64 = b.try_into().unwrap_or(0);
    if b_u64 == 0 { Ok(Value::nat_small(0)) } else { Ok(Value::nat_small(a_u64 / b_u64)) }
}

fn uint64_mod(args: &[Value]) -> InterpResult<Value> {
    let (a, b) = find_two_nats(args)?;
    let a_u64: u64 = a.try_into().unwrap_or(0);
    let b_u64: u64 = b.try_into().unwrap_or(0);
    if b_u64 == 0 { Ok(Value::nat_small(0)) } else { Ok(Value::nat_small(a_u64 % b_u64)) }
}

fn uint64_dec_eq(args: &[Value]) -> InterpResult<Value> {
    let (a, b) = find_two_nats(args)?;
    Ok(make_decidable(a == b))
}

fn uint64_dec_lt(args: &[Value]) -> InterpResult<Value> {
    let (a, b) = find_two_nats(args)?;
    Ok(make_decidable(a < b))
}

fn uint64_dec_le(args: &[Value]) -> InterpResult<Value> {
    let (a, b) = find_two_nats(args)?;
    Ok(make_decidable(a <= b))
}

// --------------- UInt8 / UInt16 builtins ---------------

fn uint8_of_nat(args: &[Value]) -> InterpResult<Value> {
    let n = find_last_nat_val(args)?;
    let v: u64 = n.try_into().unwrap_or(0);
    Ok(Value::nat_small(v % 256))
}

fn uint8_to_nat(args: &[Value]) -> InterpResult<Value> {
    let n = find_last_nat_val(args)?;
    Ok(Value::nat(n.clone()))
}

fn uint16_of_nat(args: &[Value]) -> InterpResult<Value> {
    let n = find_last_nat_val(args)?;
    let v: u64 = n.try_into().unwrap_or(0);
    Ok(Value::nat_small(v % 65536))
}

fn uint16_to_nat(args: &[Value]) -> InterpResult<Value> {
    let n = find_last_nat_val(args)?;
    Ok(Value::nat(n.clone()))
}

// --------------- Char builtins ---------------

fn char_of_nat(args: &[Value]) -> InterpResult<Value> {
    let n = find_last_nat_val(args)?;
    let code: u32 = n.try_into().unwrap_or(0);
    // Valid Unicode scalar value, or replacement character
    let ch = char::from_u32(code).unwrap_or('\u{FFFD}');
    Ok(Value::nat_small(ch as u64))
}

fn char_to_nat(args: &[Value]) -> InterpResult<Value> {
    let n = find_last_nat_val(args)?;
    Ok(Value::nat(n.clone()))
}

// --------------- Monadic helpers ---------------

/// Wrap a result in ST.Out.mk (ST return value).
fn st_result(val: Value, world: Value) -> Value {
    Value::Ctor {
        tag: 0,
        name: Name::from_str_parts("EStateM.Result.ok"),
        fields: vec![val, world],
    }
}

/// Wrap a result in EStateM.Result.ok (IO/EST success).
fn io_ok(val: Value, world: Value) -> Value {
    Value::Ctor {
        tag: 0,
        name: Name::from_str_parts("EStateM.Result.ok"),
        fields: vec![val, world],
    }
}

/// Wrap an error in EStateM.Result.error (IO/EST failure).
#[allow(dead_code)]
fn io_error(err: Value, world: Value) -> Value {
    Value::Ctor {
        tag: 1,
        name: Name::from_str_parts("EStateM.Result.error"),
        fields: vec![err, world],
    }
}

/// Extract the world token (last argument).
fn extract_world(args: &[Value]) -> Value {
    args.last().cloned().unwrap_or(Value::Erased)
}

// --------------- ST/Ref builtins (monadic) ---------------

fn st_prim_mk_ref(args: &[Value]) -> InterpResult<Value> {
    // ST.Prim.mkRef : {σ α : Type} → α → ST σ (ST.Ref σ α)
    // args: [σ, α, initial_value, world_token]
    let world = extract_world(args);
    // Find the initial value (skip Erased type args and world token)
    let initial = args.iter()
        .rev()
        .skip(1) // skip world token
        .find(|v| !matches!(v, Value::Erased))
        .cloned()
        .unwrap_or(Value::Erased);
    let r = Value::Ref(Arc::new(RefCell::new(initial)));
    Ok(st_result(r, world))
}

fn st_prim_ref_get(args: &[Value]) -> InterpResult<Value> {
    // ST.Prim.Ref.get : {σ α : Type} → ST.Ref σ α → ST σ α
    // args: [σ, α, ref, world_token]
    let world = extract_world(args);
    let r = find_ref(args)?;
    Ok(st_result(r.borrow().clone(), world))
}

fn st_prim_ref_set(args: &[Value]) -> InterpResult<Value> {
    // ST.Prim.Ref.set : {σ α : Type} → ST.Ref σ α → α → ST σ PUnit
    // args: [σ, α, ref, value, world_token]
    let world = extract_world(args);
    let r = find_ref(args)?;
    // Value is the second-to-last non-erased, non-ref arg
    let val = args.iter()
        .rev()
        .skip(1) // skip world
        .find(|v| !matches!(v, Value::Erased | Value::Ref(_)))
        .cloned()
        .unwrap_or(Value::Erased);
    *r.borrow_mut() = val;
    Ok(st_result(Value::unit(), world))
}

fn st_prim_ref_swap(args: &[Value]) -> InterpResult<Value> {
    // ST.Prim.Ref.swap : {σ α : Type} → ST.Ref σ α → α → ST σ α
    let world = extract_world(args);
    let r = find_ref(args)?;
    let new_val = args.iter()
        .rev()
        .skip(1)
        .find(|v| !matches!(v, Value::Erased | Value::Ref(_)))
        .cloned()
        .unwrap_or(Value::Erased);
    let old_val = r.borrow().clone();
    *r.borrow_mut() = new_val;
    Ok(st_result(old_val, world))
}

fn st_prim_ref_modify_get(args: &[Value]) -> InterpResult<Value> {
    // For now, stub — this needs the interpreter to apply a closure
    let world = extract_world(args);
    Ok(st_result(Value::Erased, world))
}

fn find_ref(args: &[Value]) -> InterpResult<&RefCell<Value>> {
    args.iter()
        .find_map(|v| match v {
            Value::Ref(r) => Some(r.as_ref()),
            _ => None,
        })
        .ok_or_else(|| InterpError::BuiltinError("expected Ref arg".into()))
}

// --------------- IO builtins (monadic) ---------------

fn io_println(args: &[Value]) -> InterpResult<Value> {
    let world = extract_world(args);
    if let Some(s) = args.iter().find_map(|v| v.as_str()) {
        eprintln!("[IO.println] {}", s);
    }
    Ok(io_ok(Value::unit(), world))
}

fn io_print(args: &[Value]) -> InterpResult<Value> {
    let world = extract_world(args);
    if let Some(s) = args.iter().find_map(|v| v.as_str()) {
        eprint!("[IO.print] {}", s);
    }
    Ok(io_ok(Value::unit(), world))
}

fn io_eprintln(args: &[Value]) -> InterpResult<Value> {
    let world = extract_world(args);
    if let Some(s) = args.iter().find_map(|v| v.as_str()) {
        eprintln!("[IO.eprintln] {}", s);
    }
    Ok(io_ok(Value::unit(), world))
}

// --------------- String (additional) builtins ---------------

fn string_data(args: &[Value]) -> InterpResult<Value> {
    // String.data : String → List Char
    // Convert string to a List of Char (represented as Nat values)
    let s = extract_str(args, 0)?;
    let mut result = Value::Ctor {
        tag: 0,
        name: Name::from_str_parts("List.nil"),
        fields: vec![],
    };
    for ch in s.chars().rev() {
        result = Value::Ctor {
            tag: 1,
            name: Name::from_str_parts("List.cons"),
            fields: vec![Value::nat_small(ch as u64), result],
        };
    }
    Ok(result)
}

fn string_intercalate(args: &[Value]) -> InterpResult<Value> {
    // String.intercalate : String → List String → String
    let strs: Vec<&str> = args.iter().filter_map(|v| v.as_str()).collect();
    if strs.len() >= 2 {
        // Simple case: just join with separator
        Ok(Value::string(strs[1..].join(strs[0])))
    } else {
        Ok(Value::string(""))
    }
}

fn string_is_empty(args: &[Value]) -> InterpResult<Value> {
    let s = extract_str(args, 0)?;
    Ok(Value::bool_(s.is_empty()))
}

fn string_get(args: &[Value]) -> InterpResult<Value> {
    // String.get : String → Pos → Char
    let s = extract_str(args, 0)?;
    let idx = find_last_nat(args)?;
    let idx_usize: usize = idx.try_into().unwrap_or(0);
    let ch = s.chars().nth(idx_usize).unwrap_or('\0');
    Ok(Value::nat_small(ch as u64))
}

fn string_take(args: &[Value]) -> InterpResult<Value> {
    let s = extract_str(args, 0)?;
    let n = find_last_nat(args)?;
    let n_usize: usize = n.try_into().unwrap_or(0);
    let result: String = s.chars().take(n_usize).collect();
    Ok(Value::string(result.as_str()))
}

fn string_drop(args: &[Value]) -> InterpResult<Value> {
    let s = extract_str(args, 0)?;
    let n = find_last_nat(args)?;
    let n_usize: usize = n.try_into().unwrap_or(0);
    let result: String = s.chars().skip(n_usize).collect();
    Ok(Value::string(result.as_str()))
}

fn string_trim_right(args: &[Value]) -> InterpResult<Value> {
    let s = extract_str(args, 0)?;
    Ok(Value::string(s.trim_end()))
}

// --------------- Thunk builtins ---------------

fn thunk_pure(args: &[Value]) -> InterpResult<Value> {
    // Thunk.pure : {α : Type} → α → Thunk α
    // Eager evaluation: just return the value
    args.iter()
        .rev()
        .find(|v| !matches!(v, Value::Erased))
        .cloned()
        .ok_or_else(|| InterpError::BuiltinError("Thunk.pure: missing value".into()))
}

fn thunk_get(args: &[Value]) -> InterpResult<Value> {
    // Thunk.get : {α : Type} → Thunk α → α
    args.iter()
        .rev()
        .find(|v| !matches!(v, Value::Erased))
        .cloned()
        .ok_or_else(|| InterpError::BuiltinError("Thunk.get: missing thunk".into()))
}

// --------------- Platform builtins ---------------

fn platform_get_is_windows(args: &[Value]) -> InterpResult<Value> {
    let world = extract_world(args);
    Ok(io_ok(Value::bool_(false), world))
}

fn platform_get_is_osx(args: &[Value]) -> InterpResult<Value> {
    let world = extract_world(args);
    Ok(io_ok(Value::bool_(cfg!(target_os = "macos")), world))
}

fn platform_get_is_emscripten(args: &[Value]) -> InterpResult<Value> {
    let world = extract_world(args);
    Ok(io_ok(Value::bool_(false), world))
}

fn platform_get_num_bits(args: &[Value]) -> InterpResult<Value> {
    // System.Platform.getNumBits : Unit → USize
    // Non-monadic, returns 64
    let _ = args;
    Ok(Value::nat_small(64))
}

// --------------- IO timing/heartbeat stubs ---------------

fn io_mono_ms_now(args: &[Value]) -> InterpResult<Value> {
    let world = extract_world(args);
    Ok(io_ok(Value::nat_small(0), world))
}

fn io_mono_nanos_now(args: &[Value]) -> InterpResult<Value> {
    let world = extract_world(args);
    Ok(io_ok(Value::nat_small(0), world))
}

fn io_get_num_heartbeats(args: &[Value]) -> InterpResult<Value> {
    let world = extract_world(args);
    Ok(io_ok(Value::nat_small(0), world))
}

fn io_initializing(args: &[Value]) -> InterpResult<Value> {
    let world = extract_world(args);
    Ok(io_ok(Value::bool_(false), world))
}

// --------------- HashMap builtins ---------------

fn value_hash(v: &Value) -> u64 {
    let mut hasher = DefaultHasher::new();
    match v {
        Value::Nat(n) => { 0u8.hash(&mut hasher); n.to_string().hash(&mut hasher); }
        Value::String(s) => { 1u8.hash(&mut hasher); s.hash(&mut hasher); }
        Value::Ctor { tag, name, .. } => { 2u8.hash(&mut hasher); tag.hash(&mut hasher); name.to_string().hash(&mut hasher); }
        _ => { 3u8.hash(&mut hasher); }
    }
    hasher.finish()
}

fn value_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Nat(x), Value::Nat(y)) => x == y,
        (Value::String(x), Value::String(y)) => x == y,
        (Value::Ctor { tag: t1, name: n1, fields: f1 }, Value::Ctor { tag: t2, name: n2, fields: f2 }) => {
            t1 == t2 && n1 == n2 && f1.len() == f2.len() && f1.iter().zip(f2).all(|(a, b)| value_eq(a, b))
        }
        _ => false,
    }
}

fn hashmap_mk_empty(args: &[Value]) -> InterpResult<Value> {
    let _ = args;
    Ok(Value::HashMap(Arc::new(RefCell::new(FxHashMap::default()))))
}

fn hashmap_insert(args: &[Value]) -> InterpResult<Value> {
    // Lean.HashMap.insert : {α β : Type} → HashMap α β → α → β → HashMap α β
    // Find HashMap, key, value among args
    let map = find_hashmap(args)?;
    let non_erased: Vec<&Value> = args.iter()
        .filter(|v| !matches!(v, Value::Erased | Value::HashMap(_)))
        .collect();
    if non_erased.len() >= 2 {
        let key = non_erased[non_erased.len() - 2];
        let val = non_erased[non_erased.len() - 1];
        let h = value_hash(key);
        let mut m = map.borrow_mut();
        let bucket = m.entry(h).or_default();
        // Update if key exists, otherwise insert
        if let Some(entry) = bucket.iter_mut().find(|(k, _)| value_eq(k, key)) {
            entry.1 = val.clone();
        } else {
            bucket.push((key.clone(), val.clone()));
        }
    }
    Ok(Value::HashMap(Arc::new(RefCell::new(map.borrow().clone()))))
}

fn hashmap_find(args: &[Value]) -> InterpResult<Value> {
    // Lean.HashMap.find? : {α β : Type} → HashMap α β → α → Option β
    let map = find_hashmap(args)?;
    let key = args.iter()
        .rev()
        .find(|v| !matches!(v, Value::Erased | Value::HashMap(_)))
        .ok_or_else(|| InterpError::BuiltinError("HashMap.find?: missing key".into()))?;
    let h = value_hash(key);
    let m = map.borrow();
    if let Some(bucket) = m.get(&h) {
        if let Some((_, v)) = bucket.iter().find(|(k, _)| value_eq(k, key)) {
            return Ok(Value::some(v.clone()));
        }
    }
    Ok(Value::none())
}

fn hashmap_size(args: &[Value]) -> InterpResult<Value> {
    let map = find_hashmap(args)?;
    let total: usize = map.borrow().values().map(|b| b.len()).sum();
    Ok(Value::nat_small(total as u64))
}

fn hashmap_contains(args: &[Value]) -> InterpResult<Value> {
    let map = find_hashmap(args)?;
    let key = args.iter()
        .rev()
        .find(|v| !matches!(v, Value::Erased | Value::HashMap(_)))
        .ok_or_else(|| InterpError::BuiltinError("HashMap.contains: missing key".into()))?;
    let h = value_hash(key);
    let m = map.borrow();
    let found = m.get(&h).is_some_and(|bucket| bucket.iter().any(|(k, _)| value_eq(k, key)));
    Ok(Value::bool_(found))
}

fn find_hashmap(args: &[Value]) -> InterpResult<&RefCell<crate::value::HashMapBuckets>> {
    args.iter()
        .find_map(|v| match v {
            Value::HashMap(m) => Some(m.as_ref()),
            _ => None,
        })
        .ok_or_else(|| InterpError::BuiltinError("expected HashMap arg".into()))
}

// --------------- ByteArray builtins ---------------

fn bytearray_mk_empty(args: &[Value]) -> InterpResult<Value> {
    let _ = args;
    Ok(Value::ByteArray(Arc::new(Vec::new())))
}

fn bytearray_push(args: &[Value]) -> InterpResult<Value> {
    // ByteArray.push : ByteArray → UInt8 → ByteArray
    let ba = find_bytearray(args)?;
    let byte = find_last_nat(args)?;
    let b: u8 = byte.try_into().unwrap_or(0);
    let mut new_ba = ba.clone();
    new_ba.push(b);
    Ok(Value::ByteArray(Arc::new(new_ba)))
}

fn bytearray_size(args: &[Value]) -> InterpResult<Value> {
    let ba = find_bytearray(args)?;
    Ok(Value::nat_small(ba.len() as u64))
}

fn bytearray_get_bang(args: &[Value]) -> InterpResult<Value> {
    let ba = find_bytearray(args)?;
    let idx = find_last_nat(args)?;
    let idx_usize: usize = idx.try_into().unwrap_or(0);
    let byte = ba.get(idx_usize).copied().unwrap_or(0);
    Ok(Value::nat_small(byte as u64))
}

fn find_bytearray(args: &[Value]) -> InterpResult<&Vec<u8>> {
    args.iter()
        .find_map(|v| match v {
            Value::ByteArray(ba) => Some(ba.as_ref()),
            _ => None,
        })
        .ok_or_else(|| InterpError::BuiltinError("expected ByteArray arg".into()))
}

// --------------- Additional Array builtins ---------------

fn array_fget(args: &[Value]) -> InterpResult<Value> {
    // Array.fget : {α : Type} → (a : Array α) → Fin a.size → α
    let arr = find_array(args)?;
    let idx = find_last_nat(args)?;
    let idx_usize: usize = idx.try_into().unwrap_or(0);
    arr.get(idx_usize)
        .cloned()
        .ok_or_else(|| InterpError::BuiltinError(format!("Array.fget: index {} out of bounds", idx)))
}

fn array_fset(args: &[Value]) -> InterpResult<Value> {
    // Array.fset : {α : Type} → (a : Array α) → Fin a.size → α → Array α
    let arr = find_array(args)?;
    let mut new_arr = arr.clone();
    let nats: Vec<&BigUint> = args.iter().filter_map(|v| v.as_nat()).collect();
    if let Some(idx) = nats.first() {
        let idx_usize: usize = (*idx).try_into().unwrap_or(0);
        if let Some(val) = args.iter().rev().find(|v| {
            !matches!(v, Value::Nat(_) | Value::Array(_) | Value::Erased)
        }) {
            if idx_usize < new_arr.len() {
                new_arr[idx_usize] = val.clone();
            }
        }
    }
    Ok(Value::Array(Arc::new(new_arr)))
}

fn array_pop(args: &[Value]) -> InterpResult<Value> {
    let arr = find_array(args)?;
    let mut new_arr = arr.clone();
    new_arr.pop();
    Ok(Value::Array(Arc::new(new_arr)))
}

fn array_fswap(args: &[Value]) -> InterpResult<Value> {
    // Array.fswap : {α : Type} → (a : Array α) → Fin a.size → Fin a.size → Array α
    let arr = find_array(args)?;
    let nats: Vec<usize> = args.iter()
        .filter_map(|v| v.as_nat())
        .filter_map(|n| n.try_into().ok())
        .collect();
    let mut new_arr = arr.clone();
    if nats.len() >= 2 && nats[0] < new_arr.len() && nats[1] < new_arr.len() {
        new_arr.swap(nats[0], nats[1]);
    }
    Ok(Value::Array(Arc::new(new_arr)))
}

fn array_uget(args: &[Value]) -> InterpResult<Value> {
    // Array.uget : {α : Type} → (a : Array α) → USize → α
    let arr = find_array(args)?;
    let idx = find_last_nat(args)?;
    let idx_usize: usize = idx.try_into().unwrap_or(0);
    arr.get(idx_usize)
        .cloned()
        .ok_or_else(|| InterpError::BuiltinError(format!("Array.uget: index {} out of bounds", idx)))
}

// --------------- Int builtins ---------------

fn extract_int(v: &Value) -> Option<BigInt> {
    v.to_bigint()
}

fn find_two_ints(args: &[Value]) -> InterpResult<(BigInt, BigInt)> {
    let ints: Vec<BigInt> = args.iter().filter_map(extract_int).collect();
    if ints.len() >= 2 {
        Ok((ints[0].clone(), ints[1].clone()))
    } else {
        Err(InterpError::BuiltinError(format!(
            "expected 2 Int args, found {}",
            ints.len()
        )))
    }
}

fn int_of_nat(args: &[Value]) -> InterpResult<Value> {
    let n = find_last_nat_val(args)?;
    Ok(Value::Int(Arc::new(BigInt::from(n.clone()))))
}

fn int_neg_succ(args: &[Value]) -> InterpResult<Value> {
    let n = find_last_nat_val(args)?;
    Ok(Value::Int(Arc::new(-(BigInt::from(n.clone()) + BigInt::one()))))
}

fn int_add(args: &[Value]) -> InterpResult<Value> {
    let (a, b) = find_two_ints(args)?;
    Ok(Value::Int(Arc::new(a + b)))
}

fn int_sub(args: &[Value]) -> InterpResult<Value> {
    let (a, b) = find_two_ints(args)?;
    Ok(Value::Int(Arc::new(a - b)))
}

fn int_mul(args: &[Value]) -> InterpResult<Value> {
    let (a, b) = find_two_ints(args)?;
    Ok(Value::Int(Arc::new(a * b)))
}

fn int_div(args: &[Value]) -> InterpResult<Value> {
    let (a, b) = find_two_ints(args)?;
    if b.is_zero() {
        Ok(Value::Int(Arc::new(BigInt::zero())))
    } else {
        // Lean uses truncated division
        Ok(Value::Int(Arc::new(a / &b)))
    }
}

fn int_mod(args: &[Value]) -> InterpResult<Value> {
    let (a, b) = find_two_ints(args)?;
    if b.is_zero() {
        Ok(Value::Int(Arc::new(BigInt::zero())))
    } else {
        Ok(Value::Int(Arc::new(a % &b)))
    }
}

fn int_neg(args: &[Value]) -> InterpResult<Value> {
    let ints: Vec<BigInt> = args.iter().filter_map(extract_int).collect();
    let a = ints.first().ok_or_else(|| InterpError::BuiltinError("expected Int arg".into()))?;
    Ok(Value::Int(Arc::new(-a)))
}

fn int_dec_eq(args: &[Value]) -> InterpResult<Value> {
    let (a, b) = find_two_ints(args)?;
    Ok(make_decidable(a == b))
}

fn int_dec_le(args: &[Value]) -> InterpResult<Value> {
    let (a, b) = find_two_ints(args)?;
    Ok(make_decidable(a <= b))
}

fn int_dec_lt(args: &[Value]) -> InterpResult<Value> {
    let (a, b) = find_two_ints(args)?;
    Ok(make_decidable(a < b))
}

fn int_dec_nonneg(args: &[Value]) -> InterpResult<Value> {
    let ints: Vec<BigInt> = args.iter().filter_map(extract_int).collect();
    let a = ints.first().ok_or_else(|| InterpError::BuiltinError("expected Int arg".into()))?;
    Ok(make_decidable(!a.is_negative()))
}

fn int_to_nat(args: &[Value]) -> InterpResult<Value> {
    let ints: Vec<BigInt> = args.iter().filter_map(extract_int).collect();
    let a = ints.first().ok_or_else(|| InterpError::BuiltinError("expected Int arg".into()))?;
    if a.is_negative() {
        Ok(Value::nat_small(0))
    } else {
        Ok(Value::nat(a.to_biguint().unwrap_or_default()))
    }
}

// --------------- Name builtins ---------------

fn find_name(args: &[Value]) -> Option<&Name> {
    args.iter().find_map(|v| match v {
        Value::Ctor { name, .. } => Some(name),
        _ => None,
    })
}

fn name_beq(args: &[Value]) -> InterpResult<Value> {
    // Name values might be represented as KernelExpr or as constructors
    // For now, compare string representations
    let names: Vec<String> = args.iter().filter_map(|v| {
        match v {
            Value::Ctor { name, .. } => Some(name.to_string()),
            _ => None,
        }
    }).collect();
    if names.len() >= 2 {
        Ok(Value::bool_(names[0] == names[1]))
    } else {
        Ok(Value::bool_(false))
    }
}

fn name_hash(args: &[Value]) -> InterpResult<Value> {
    if let Some(name) = find_name(args) {
        let mut hasher = DefaultHasher::new();
        name.to_string().hash(&mut hasher);
        Ok(Value::nat_small(hasher.finish()))
    } else {
        Ok(Value::nat_small(0))
    }
}

fn name_mk_str(args: &[Value]) -> InterpResult<Value> {
    // Name.mkStr : Name → String → Name
    // This operates on the Name inductive type; for now stub
    let _ = args;
    Ok(Value::Erased)
}

fn name_mk_num(args: &[Value]) -> InterpResult<Value> {
    let _ = args;
    Ok(Value::Erased)
}

// --------------- USize additional builtins ---------------

const USIZE_SIZE: u128 = 1u128 << 64; // 64-bit platform

fn usize_add(args: &[Value]) -> InterpResult<Value> {
    let (a, b) = find_two_nats(args)?;
    let a_u64: u64 = a.try_into().unwrap_or(0);
    let b_u64: u64 = b.try_into().unwrap_or(0);
    let result = ((a_u64 as u128 + b_u64 as u128) % USIZE_SIZE) as u64;
    Ok(Value::nat_small(result))
}

fn usize_sub(args: &[Value]) -> InterpResult<Value> {
    let (a, b) = find_two_nats(args)?;
    let a_u64: u64 = a.try_into().unwrap_or(0);
    let b_u64: u64 = b.try_into().unwrap_or(0);
    Ok(Value::nat_small(a_u64.wrapping_sub(b_u64)))
}

fn usize_mul(args: &[Value]) -> InterpResult<Value> {
    let (a, b) = find_two_nats(args)?;
    let a_u64: u64 = a.try_into().unwrap_or(0);
    let b_u64: u64 = b.try_into().unwrap_or(0);
    let result = ((a_u64 as u128 * b_u64 as u128) % USIZE_SIZE) as u64;
    Ok(Value::nat_small(result))
}

fn usize_div(args: &[Value]) -> InterpResult<Value> {
    let (a, b) = find_two_nats(args)?;
    let a_u64: u64 = a.try_into().unwrap_or(0);
    let b_u64: u64 = b.try_into().unwrap_or(0);
    if b_u64 == 0 { Ok(Value::nat_small(0)) } else { Ok(Value::nat_small(a_u64 / b_u64)) }
}

fn usize_mod(args: &[Value]) -> InterpResult<Value> {
    let (a, b) = find_two_nats(args)?;
    let a_u64: u64 = a.try_into().unwrap_or(0);
    let b_u64: u64 = b.try_into().unwrap_or(0);
    if b_u64 == 0 { Ok(Value::nat_small(0)) } else { Ok(Value::nat_small(a_u64 % b_u64)) }
}

fn usize_dec_eq(args: &[Value]) -> InterpResult<Value> {
    let (a, b) = find_two_nats(args)?;
    Ok(make_decidable(a == b))
}

fn usize_dec_lt(args: &[Value]) -> InterpResult<Value> {
    let (a, b) = find_two_nats(args)?;
    Ok(make_decidable(a < b))
}

fn usize_dec_le(args: &[Value]) -> InterpResult<Value> {
    let (a, b) = find_two_nats(args)?;
    Ok(make_decidable(a <= b))
}

// --------------- Float stubs ---------------

fn float_of_scientific(args: &[Value]) -> InterpResult<Value> {
    // Float.ofScientific : Nat → Bool → Nat → Float
    // Stub: represent as Nat(0) for now
    let _ = args;
    Ok(Value::nat_small(0))
}

fn float_to_string(args: &[Value]) -> InterpResult<Value> {
    let _ = args;
    Ok(Value::string("0.0"))
}

// --------------- Lean.Expr structural builtins ---------------
// Lean.Expr is a regular inductive type; values arrive as Value::Ctor.
// We extract tag/fields and operate structurally.

/// Lean.Expr tag indices (matching the Lean 4 inductive order):
/// bvar=0, fvar=1, mvar=2, sort=3, const=4, app=5, lam=6, forall=7, letE=8, lit=9, mdata=10, proj=11
fn lean_expr_eqv(args: &[Value]) -> InterpResult<Value> {
    // Lean.Expr.eqv (a b : Expr) : Bool — structural equality
    let non_erased: Vec<&Value> = args.iter().filter(|v| !matches!(v, Value::Erased)).collect();
    if non_erased.len() >= 2 {
        Ok(Value::bool_(value_eq(non_erased[0], non_erased[1])))
    } else {
        Ok(Value::bool_(false))
    }
}

fn lean_expr_lt(args: &[Value]) -> InterpResult<Value> {
    // Lean.Expr.lt (a b : Expr) : Bool — structural ordering
    let non_erased: Vec<&Value> = args.iter().filter(|v| !matches!(v, Value::Erased)).collect();
    if non_erased.len() >= 2 {
        Ok(Value::bool_(value_lt(non_erased[0], non_erased[1])))
    } else {
        Ok(Value::bool_(false))
    }
}

fn lean_expr_hash(args: &[Value]) -> InterpResult<Value> {
    // Lean.Expr.hash (a : Expr) : USize
    let v = args.iter().find(|v| !matches!(v, Value::Erased)).unwrap_or(&Value::Erased);
    Ok(Value::nat_small(value_hash(v)))
}

/// Get the tag index of a Value::Ctor (for Lean inductive dispatching).
fn ctor_tag(v: &Value) -> Option<u32> {
    match v {
        Value::Ctor { tag, .. } => Some(*tag),
        _ => None,
    }
}

/// Get a field of a Value::Ctor by index.
fn ctor_field(v: &Value, idx: usize) -> Option<&Value> {
    match v {
        Value::Ctor { fields, .. } => fields.get(idx),
        _ => None,
    }
}

fn lean_expr_is_bvar(args: &[Value]) -> InterpResult<Value> {
    let v = args.iter().find(|v| !matches!(v, Value::Erased)).unwrap_or(&Value::Erased);
    Ok(Value::bool_(ctor_tag(v) == Some(0)))
}
fn lean_expr_is_fvar(args: &[Value]) -> InterpResult<Value> {
    let v = args.iter().find(|v| !matches!(v, Value::Erased)).unwrap_or(&Value::Erased);
    Ok(Value::bool_(ctor_tag(v) == Some(1)))
}
fn lean_expr_is_mvar(args: &[Value]) -> InterpResult<Value> {
    let v = args.iter().find(|v| !matches!(v, Value::Erased)).unwrap_or(&Value::Erased);
    Ok(Value::bool_(ctor_tag(v) == Some(2)))
}
fn lean_expr_is_sort(args: &[Value]) -> InterpResult<Value> {
    let v = args.iter().find(|v| !matches!(v, Value::Erased)).unwrap_or(&Value::Erased);
    Ok(Value::bool_(ctor_tag(v) == Some(3)))
}
fn lean_expr_is_const(args: &[Value]) -> InterpResult<Value> {
    let v = args.iter().find(|v| !matches!(v, Value::Erased)).unwrap_or(&Value::Erased);
    Ok(Value::bool_(ctor_tag(v) == Some(4)))
}
fn lean_expr_is_app(args: &[Value]) -> InterpResult<Value> {
    let v = args.iter().find(|v| !matches!(v, Value::Erased)).unwrap_or(&Value::Erased);
    Ok(Value::bool_(ctor_tag(v) == Some(5)))
}
fn lean_expr_is_lambda(args: &[Value]) -> InterpResult<Value> {
    let v = args.iter().find(|v| !matches!(v, Value::Erased)).unwrap_or(&Value::Erased);
    Ok(Value::bool_(ctor_tag(v) == Some(6)))
}
fn lean_expr_is_forall(args: &[Value]) -> InterpResult<Value> {
    let v = args.iter().find(|v| !matches!(v, Value::Erased)).unwrap_or(&Value::Erased);
    Ok(Value::bool_(ctor_tag(v) == Some(7)))
}
fn lean_expr_is_let(args: &[Value]) -> InterpResult<Value> {
    let v = args.iter().find(|v| !matches!(v, Value::Erased)).unwrap_or(&Value::Erased);
    Ok(Value::bool_(ctor_tag(v) == Some(8)))
}
fn lean_expr_is_lit(args: &[Value]) -> InterpResult<Value> {
    let v = args.iter().find(|v| !matches!(v, Value::Erased)).unwrap_or(&Value::Erased);
    Ok(Value::bool_(ctor_tag(v) == Some(9)))
}
fn lean_expr_is_mdata(args: &[Value]) -> InterpResult<Value> {
    let v = args.iter().find(|v| !matches!(v, Value::Erased)).unwrap_or(&Value::Erased);
    Ok(Value::bool_(ctor_tag(v) == Some(10)))
}
fn lean_expr_is_proj(args: &[Value]) -> InterpResult<Value> {
    let v = args.iter().find(|v| !matches!(v, Value::Erased)).unwrap_or(&Value::Erased);
    Ok(Value::bool_(ctor_tag(v) == Some(11)))
}

fn lean_expr_bvar_idx(args: &[Value]) -> InterpResult<Value> {
    // Returns the bvar index from a bvar Expr
    let v = args.iter().find(|v| matches!(v, Value::Ctor { tag: 0, .. })).unwrap_or(&Value::Erased);
    Ok(ctor_field(v, 0).cloned().unwrap_or(Value::nat_small(0)))
}

fn lean_expr_fvar_id(args: &[Value]) -> InterpResult<Value> {
    let v = args.iter().find(|v| matches!(v, Value::Ctor { tag: 1, .. })).unwrap_or(&Value::Erased);
    Ok(ctor_field(v, 0).cloned().unwrap_or(Value::Erased))
}

fn lean_expr_mvar_id(args: &[Value]) -> InterpResult<Value> {
    let v = args.iter().find(|v| matches!(v, Value::Ctor { tag: 2, .. })).unwrap_or(&Value::Erased);
    Ok(ctor_field(v, 0).cloned().unwrap_or(Value::Erased))
}

fn lean_expr_has_loose_bvars(args: &[Value]) -> InterpResult<Value> {
    // Stub: return false (conservative approximation)
    let _ = args;
    Ok(Value::bool_(false))
}

fn lean_expr_loose_bvar_range(args: &[Value]) -> InterpResult<Value> {
    // Stub: return 0
    let _ = args;
    Ok(Value::nat_small(0))
}

fn lean_expr_has_fvar(args: &[Value]) -> InterpResult<Value> {
    let _ = args;
    Ok(Value::bool_(false))
}

fn lean_expr_has_mvar(args: &[Value]) -> InterpResult<Value> {
    let _ = args;
    Ok(Value::bool_(false))
}

fn lean_expr_approx_depth(args: &[Value]) -> InterpResult<Value> {
    let _ = args;
    Ok(Value::nat_small(1))
}

fn lean_expr_head_beta(args: &[Value]) -> InterpResult<Value> {
    // Stub: return the expr unchanged
    let v = args.iter().find(|v| !matches!(v, Value::Erased)).cloned().unwrap_or(Value::Erased);
    Ok(v)
}

fn lean_expr_get_app_num_args(args: &[Value]) -> InterpResult<Value> {
    // Count application spine depth
    fn count_apps(v: &Value) -> u64 {
        match v {
            Value::Ctor { tag: 5, fields, .. } if fields.len() >= 2 => {
                1 + count_apps(&fields[0])
            }
            _ => 0,
        }
    }
    let v = args.iter().find(|v| !matches!(v, Value::Erased)).unwrap_or(&Value::Erased);
    Ok(Value::nat_small(count_apps(v)))
}

fn lean_expr_ctor_idx(args: &[Value]) -> InterpResult<Value> {
    let v = args.iter().find(|v| !matches!(v, Value::Erased)).unwrap_or(&Value::Erased);
    Ok(Value::nat_small(ctor_tag(v).unwrap_or(0) as u64))
}

fn lean_expr_dbg_to_string(args: &[Value]) -> InterpResult<Value> {
    let v = args.iter().find(|v| !matches!(v, Value::Erased)).unwrap_or(&Value::Erased);
    Ok(Value::string(format!("{:?}", v).as_str()))
}

/// Build a Lean.Expr.bvar Ctor value from a Nat idx.
fn lean_mk_bvar(args: &[Value]) -> InterpResult<Value> {
    let idx = find_last_nat_val(args)?;
    Ok(Value::Ctor {
        tag: 0,
        name: Name::from_str_parts("Lean.Expr.bvar"),
        fields: vec![Value::nat(idx.clone())],
    })
}

fn lean_mk_fvar(args: &[Value]) -> InterpResult<Value> {
    let id = args.iter().rev().find(|v| !matches!(v, Value::Erased)).cloned().unwrap_or(Value::Erased);
    Ok(Value::Ctor {
        tag: 1,
        name: Name::from_str_parts("Lean.Expr.fvar"),
        fields: vec![id],
    })
}

fn lean_mk_mvar(args: &[Value]) -> InterpResult<Value> {
    let id = args.iter().rev().find(|v| !matches!(v, Value::Erased)).cloned().unwrap_or(Value::Erased);
    Ok(Value::Ctor {
        tag: 2,
        name: Name::from_str_parts("Lean.Expr.mvar"),
        fields: vec![id],
    })
}

fn lean_mk_sort(args: &[Value]) -> InterpResult<Value> {
    let lvl = args.iter().rev().find(|v| !matches!(v, Value::Erased)).cloned().unwrap_or(Value::Erased);
    Ok(Value::Ctor {
        tag: 3,
        name: Name::from_str_parts("Lean.Expr.sort"),
        fields: vec![lvl],
    })
}

fn lean_mk_const(args: &[Value]) -> InterpResult<Value> {
    // mkConst (name : Name) (levels : List Level) : Expr
    let non_erased: Vec<Value> = args.iter().filter(|v| !matches!(v, Value::Erased)).cloned().collect();
    let (nm, levels) = match non_erased.as_slice() {
        [nm, lvls, ..] => (nm.clone(), lvls.clone()),
        [nm] => (nm.clone(), Value::Ctor { tag: 0, name: Name::from_str_parts("List.nil"), fields: vec![] }),
        [] => (Value::Erased, Value::Ctor { tag: 0, name: Name::from_str_parts("List.nil"), fields: vec![] }),
    };
    Ok(Value::Ctor {
        tag: 4,
        name: Name::from_str_parts("Lean.Expr.const"),
        fields: vec![nm, levels],
    })
}

fn lean_mk_app(args: &[Value]) -> InterpResult<Value> {
    // mkApp (f a : Expr) : Expr
    let non_erased: Vec<Value> = args.iter().filter(|v| !matches!(v, Value::Erased)).cloned().collect();
    let (f, a) = match non_erased.as_slice() {
        [f, a, ..] => (f.clone(), a.clone()),
        [f] => (f.clone(), Value::Erased),
        [] => (Value::Erased, Value::Erased),
    };
    Ok(Value::Ctor {
        tag: 5,
        name: Name::from_str_parts("Lean.Expr.app"),
        fields: vec![f, a],
    })
}

fn lean_mk_app2(args: &[Value]) -> InterpResult<Value> {
    // mkApp2 f a b = mkApp (mkApp f a) b
    let non_erased: Vec<Value> = args.iter().filter(|v| !matches!(v, Value::Erased)).cloned().collect();
    match non_erased.as_slice() {
        [f, a, b, ..] => {
            let inner = Value::Ctor { tag: 5, name: Name::from_str_parts("Lean.Expr.app"), fields: vec![f.clone(), a.clone()] };
            Ok(Value::Ctor { tag: 5, name: Name::from_str_parts("Lean.Expr.app"), fields: vec![inner, b.clone()] })
        }
        _ => Ok(Value::Erased),
    }
}

fn lean_mk_app3(args: &[Value]) -> InterpResult<Value> {
    let non_erased: Vec<Value> = args.iter().filter(|v| !matches!(v, Value::Erased)).cloned().collect();
    match non_erased.as_slice() {
        [f, a, b, c, ..] => {
            let app1 = Value::Ctor { tag: 5, name: Name::from_str_parts("Lean.Expr.app"), fields: vec![f.clone(), a.clone()] };
            let app2 = Value::Ctor { tag: 5, name: Name::from_str_parts("Lean.Expr.app"), fields: vec![app1, b.clone()] };
            Ok(Value::Ctor { tag: 5, name: Name::from_str_parts("Lean.Expr.app"), fields: vec![app2, c.clone()] })
        }
        _ => Ok(Value::Erased),
    }
}

fn lean_mk_app_n(args: &[Value]) -> InterpResult<Value> {
    // mkAppN (f : Expr) (args : Array Expr) : Expr
    let f = args.iter().find(|v| !matches!(v, Value::Erased | Value::Array(_))).cloned().unwrap_or(Value::Erased);
    let arr = args.iter().find_map(|v| match v { Value::Array(a) => Some(a.clone()), _ => None });
    let mut result = f;
    if let Some(arr) = arr {
        for arg in arr.iter() {
            result = Value::Ctor { tag: 5, name: Name::from_str_parts("Lean.Expr.app"), fields: vec![result, arg.clone()] };
        }
    }
    Ok(result)
}

fn lean_mk_lambda(args: &[Value]) -> InterpResult<Value> {
    // mkLambda (n : Name) (bi : BinderInfo) (t b : Expr) : Expr
    let non_erased: Vec<Value> = args.iter().filter(|v| !matches!(v, Value::Erased)).cloned().collect();
    match non_erased.as_slice() {
        [n, bi, t, b, ..] => Ok(Value::Ctor {
            tag: 6,
            name: Name::from_str_parts("Lean.Expr.lam"),
            fields: vec![n.clone(), bi.clone(), t.clone(), b.clone()],
        }),
        _ => Ok(Value::Erased),
    }
}

fn lean_mk_forall(args: &[Value]) -> InterpResult<Value> {
    let non_erased: Vec<Value> = args.iter().filter(|v| !matches!(v, Value::Erased)).cloned().collect();
    match non_erased.as_slice() {
        [n, bi, t, b, ..] => Ok(Value::Ctor {
            tag: 7,
            name: Name::from_str_parts("Lean.Expr.forallE"),
            fields: vec![n.clone(), bi.clone(), t.clone(), b.clone()],
        }),
        _ => Ok(Value::Erased),
    }
}

fn lean_mk_let(args: &[Value]) -> InterpResult<Value> {
    let non_erased: Vec<Value> = args.iter().filter(|v| !matches!(v, Value::Erased)).cloned().collect();
    match non_erased.as_slice() {
        [n, t, v, b, ..] => Ok(Value::Ctor {
            tag: 8,
            name: Name::from_str_parts("Lean.Expr.letE"),
            fields: vec![n.clone(), t.clone(), v.clone(), b.clone()],
        }),
        _ => Ok(Value::Erased),
    }
}

fn lean_mk_lit(args: &[Value]) -> InterpResult<Value> {
    let lit = args.iter().rev().find(|v| !matches!(v, Value::Erased)).cloned().unwrap_or(Value::Erased);
    Ok(Value::Ctor {
        tag: 9,
        name: Name::from_str_parts("Lean.Expr.lit"),
        fields: vec![lit],
    })
}

fn lean_mk_mdata(args: &[Value]) -> InterpResult<Value> {
    let non_erased: Vec<Value> = args.iter().filter(|v| !matches!(v, Value::Erased)).cloned().collect();
    match non_erased.as_slice() {
        [md, e, ..] => Ok(Value::Ctor {
            tag: 10,
            name: Name::from_str_parts("Lean.Expr.mdata"),
            fields: vec![md.clone(), e.clone()],
        }),
        _ => Ok(Value::Erased),
    }
}

fn lean_mk_proj(args: &[Value]) -> InterpResult<Value> {
    let non_erased: Vec<Value> = args.iter().filter(|v| !matches!(v, Value::Erased)).cloned().collect();
    match non_erased.as_slice() {
        [sname, idx, e, ..] => Ok(Value::Ctor {
            tag: 11,
            name: Name::from_str_parts("Lean.Expr.proj"),
            fields: vec![sname.clone(), idx.clone(), e.clone()],
        }),
        _ => Ok(Value::Erased),
    }
}

/// Structural less-than on Values (for ordered data structures).
fn value_lt(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Nat(x), Value::Nat(y)) => x < y,
        (Value::String(x), Value::String(y)) => x.as_ref() < y.as_ref(),
        (Value::Ctor { tag: t1, fields: f1, .. }, Value::Ctor { tag: t2, fields: f2, .. }) => {
            if t1 != t2 { return t1 < t2; }
            for (a, b) in f1.iter().zip(f2.iter()) {
                if value_lt(a, b) { return true; }
                if value_lt(b, a) { return false; }
            }
            f1.len() < f2.len()
        }
        _ => false,
    }
}

// --------------- Lean.Name structural builtins ---------------
// Lean.Name is a regular inductive: anonymous=0, str=1, num=2

fn lean_name_beq(args: &[Value]) -> InterpResult<Value> {
    let non_erased: Vec<&Value> = args.iter().filter(|v| !matches!(v, Value::Erased)).collect();
    if non_erased.len() >= 2 {
        Ok(Value::bool_(value_eq(non_erased[0], non_erased[1])))
    } else {
        Ok(Value::bool_(false))
    }
}

fn lean_name_hash(args: &[Value]) -> InterpResult<Value> {
    let v = args.iter().find(|v| !matches!(v, Value::Erased)).unwrap_or(&Value::Erased);
    Ok(Value::nat_small(value_hash(v)))
}

fn lean_name_str(args: &[Value]) -> InterpResult<Value> {
    // Name.str (p : Name) (s : String) : Name — constructor
    let non_erased: Vec<Value> = args.iter().filter(|v| !matches!(v, Value::Erased)).cloned().collect();
    match non_erased.as_slice() {
        [p, s, ..] => Ok(Value::Ctor {
            tag: 1,
            name: Name::from_str_parts("Lean.Name.str"),
            fields: vec![p.clone(), s.clone()],
        }),
        _ => Ok(Value::Erased),
    }
}

fn lean_name_num(args: &[Value]) -> InterpResult<Value> {
    let non_erased: Vec<Value> = args.iter().filter(|v| !matches!(v, Value::Erased)).cloned().collect();
    match non_erased.as_slice() {
        [p, n, ..] => Ok(Value::Ctor {
            tag: 2,
            name: Name::from_str_parts("Lean.Name.num"),
            fields: vec![p.clone(), n.clone()],
        }),
        _ => Ok(Value::Erased),
    }
}

fn lean_name_is_anonymous(args: &[Value]) -> InterpResult<Value> {
    let v = args.iter().find(|v| !matches!(v, Value::Erased)).unwrap_or(&Value::Erased);
    Ok(Value::bool_(ctor_tag(v) == Some(0)))
}
fn lean_name_is_str(args: &[Value]) -> InterpResult<Value> {
    let v = args.iter().find(|v| !matches!(v, Value::Erased)).unwrap_or(&Value::Erased);
    Ok(Value::bool_(ctor_tag(v) == Some(1)))
}
fn lean_name_is_num(args: &[Value]) -> InterpResult<Value> {
    let v = args.iter().find(|v| !matches!(v, Value::Erased)).unwrap_or(&Value::Erased);
    Ok(Value::bool_(ctor_tag(v) == Some(2)))
}

fn lean_name_get_string(args: &[Value]) -> InterpResult<Value> {
    // Name.getString! on a str constructor: returns the string component
    let v = args.iter().find(|v| matches!(v, Value::Ctor { tag: 1, .. })).unwrap_or(&Value::Erased);
    Ok(ctor_field(v, 1).cloned().unwrap_or(Value::string("")))
}

fn lean_name_get_num(args: &[Value]) -> InterpResult<Value> {
    let v = args.iter().find(|v| matches!(v, Value::Ctor { tag: 2, .. })).unwrap_or(&Value::Erased);
    Ok(ctor_field(v, 1).cloned().unwrap_or(Value::nat_small(0)))
}

fn lean_name_append(args: &[Value]) -> InterpResult<Value> {
    // Name.append : Name → Name → Name (suffix appended to prefix)
    // For now return the last non-anonymous name
    let non_erased: Vec<&Value> = args.iter().filter(|v| !matches!(v, Value::Erased)).collect();
    match non_erased.as_slice() {
        [_a, b, ..] => Ok((*b).clone()),
        [a] => Ok((*a).clone()),
        [] => Ok(Value::Ctor { tag: 0, name: Name::from_str_parts("Lean.Name.anonymous"), fields: vec![] }),
    }
}

fn lean_name_to_string(args: &[Value]) -> InterpResult<Value> {
    // Convert a Lean.Name Ctor value to string representation
    fn name_to_str(v: &Value) -> String {
        match v {
            Value::Ctor { tag: 0, .. } => "".to_string(),
            Value::Ctor { tag: 1, fields, .. } => {
                let prefix = fields.first().map(name_to_str).unwrap_or_default();
                let s = fields.get(1).and_then(|s| s.as_str()).unwrap_or("");
                if prefix.is_empty() { s.to_string() } else { format!("{}.{}", prefix, s) }
            }
            Value::Ctor { tag: 2, fields, .. } => {
                let prefix = fields.first().map(name_to_str).unwrap_or_default();
                let n = fields.get(1).and_then(|n| n.as_nat()).map(|n| n.to_string()).unwrap_or_default();
                if prefix.is_empty() { n } else { format!("{}.{}", prefix, n) }
            }
            Value::String(s) => s.to_string(),
            _ => format!("{:?}", v),
        }
    }
    let v = args.iter().find(|v| !matches!(v, Value::Erased)).unwrap_or(&Value::Erased);
    Ok(Value::string(name_to_str(v).as_str()))
}

fn lean_name_quick_lt(args: &[Value]) -> InterpResult<Value> {
    let non_erased: Vec<&Value> = args.iter().filter(|v| !matches!(v, Value::Erased)).collect();
    if non_erased.len() >= 2 {
        Ok(Value::bool_(value_lt(non_erased[0], non_erased[1])))
    } else {
        Ok(Value::bool_(false))
    }
}

// --------------- Lean.Level structural builtins ---------------
// Lean.Level: zero=0, succ=1, max=2, imax=3, param=4, mvar=5

fn lean_level_beq(args: &[Value]) -> InterpResult<Value> {
    let non_erased: Vec<&Value> = args.iter().filter(|v| !matches!(v, Value::Erased)).collect();
    if non_erased.len() >= 2 {
        Ok(Value::bool_(value_eq(non_erased[0], non_erased[1])))
    } else {
        Ok(Value::bool_(false))
    }
}
fn lean_level_hash(args: &[Value]) -> InterpResult<Value> {
    let v = args.iter().find(|v| !matches!(v, Value::Erased)).unwrap_or(&Value::Erased);
    Ok(Value::nat_small(value_hash(v)))
}
fn lean_level_is_zero(args: &[Value]) -> InterpResult<Value> {
    let v = args.iter().find(|v| !matches!(v, Value::Erased)).unwrap_or(&Value::Erased);
    Ok(Value::bool_(ctor_tag(v) == Some(0)))
}
fn lean_level_is_succ(args: &[Value]) -> InterpResult<Value> {
    let v = args.iter().find(|v| !matches!(v, Value::Erased)).unwrap_or(&Value::Erased);
    Ok(Value::bool_(ctor_tag(v) == Some(1)))
}
fn lean_level_is_max(args: &[Value]) -> InterpResult<Value> {
    let v = args.iter().find(|v| !matches!(v, Value::Erased)).unwrap_or(&Value::Erased);
    Ok(Value::bool_(ctor_tag(v) == Some(2)))
}
fn lean_level_is_imax(args: &[Value]) -> InterpResult<Value> {
    let v = args.iter().find(|v| !matches!(v, Value::Erased)).unwrap_or(&Value::Erased);
    Ok(Value::bool_(ctor_tag(v) == Some(3)))
}
fn lean_level_is_param(args: &[Value]) -> InterpResult<Value> {
    let v = args.iter().find(|v| !matches!(v, Value::Erased)).unwrap_or(&Value::Erased);
    Ok(Value::bool_(ctor_tag(v) == Some(4)))
}
fn lean_level_is_mvar(args: &[Value]) -> InterpResult<Value> {
    let v = args.iter().find(|v| !matches!(v, Value::Erased)).unwrap_or(&Value::Erased);
    Ok(Value::bool_(ctor_tag(v) == Some(5)))
}
fn lean_level_succ_of(args: &[Value]) -> InterpResult<Value> {
    let v = args.iter().find(|v| matches!(v, Value::Ctor { tag: 1, .. })).unwrap_or(&Value::Erased);
    Ok(ctor_field(v, 0).cloned().unwrap_or(Value::Erased))
}
fn lean_level_max_of(args: &[Value]) -> InterpResult<Value> {
    let v = args.iter().find(|v| matches!(v, Value::Ctor { tag: 2, .. })).unwrap_or(&Value::Erased);
    let l = ctor_field(v, 0).cloned().unwrap_or(Value::Erased);
    let r = ctor_field(v, 1).cloned().unwrap_or(Value::Erased);
    Ok(Value::Ctor { tag: 0, name: Name::from_str_parts("Prod.mk"), fields: vec![l, r] })
}
fn lean_level_imax_of(args: &[Value]) -> InterpResult<Value> {
    let v = args.iter().find(|v| matches!(v, Value::Ctor { tag: 3, .. })).unwrap_or(&Value::Erased);
    let l = ctor_field(v, 0).cloned().unwrap_or(Value::Erased);
    let r = ctor_field(v, 1).cloned().unwrap_or(Value::Erased);
    Ok(Value::Ctor { tag: 0, name: Name::from_str_parts("Prod.mk"), fields: vec![l, r] })
}
fn lean_level_param_of(args: &[Value]) -> InterpResult<Value> {
    let v = args.iter().find(|v| matches!(v, Value::Ctor { tag: 4, .. })).unwrap_or(&Value::Erased);
    Ok(ctor_field(v, 0).cloned().unwrap_or(Value::Erased))
}
fn lean_level_mvar_of(args: &[Value]) -> InterpResult<Value> {
    let v = args.iter().find(|v| matches!(v, Value::Ctor { tag: 5, .. })).unwrap_or(&Value::Erased);
    Ok(ctor_field(v, 0).cloned().unwrap_or(Value::Erased))
}

// --------------- Lean.Environment bridge builtins ---------------

fn lean_env_find(args: &[Value]) -> InterpResult<Value> {
    // Lean.Environment.find? : Environment → Name → Option ConstantInfo
    // For now return None (the elaborator will fall back to other mechanisms)
    let _ = args;
    Ok(Value::none())
}

fn lean_env_contains(args: &[Value]) -> InterpResult<Value> {
    let _ = args;
    Ok(Value::bool_(false))
}

fn lean_env_is_constructor(args: &[Value]) -> InterpResult<Value> {
    let _ = args;
    Ok(Value::bool_(false))
}

fn lean_env_is_inductive(args: &[Value]) -> InterpResult<Value> {
    let _ = args;
    Ok(Value::bool_(false))
}

fn lean_env_is_recursor(args: &[Value]) -> InterpResult<Value> {
    let _ = args;
    Ok(Value::bool_(false))
}

// --------------- Lean.RBTree / PersistentHashMap stubs ---------------

fn lean_rbnode_depth(args: &[Value]) -> InterpResult<Value> {
    let _ = args;
    Ok(Value::nat_small(0))
}

fn lean_persistent_hashmap_mk_empty(args: &[Value]) -> InterpResult<Value> {
    let _ = args;
    Ok(Value::Array(Arc::new(Vec::new())))
}

// --------------- IO Handle stubs ---------------

fn io_get_stdout(args: &[Value]) -> InterpResult<Value> {
    let world = extract_world(args);
    Ok(io_ok(Value::nat_small(1), world)) // stdout fd = 1
}

fn io_get_stderr(args: &[Value]) -> InterpResult<Value> {
    let world = extract_world(args);
    Ok(io_ok(Value::nat_small(2), world)) // stderr fd = 2
}

fn io_get_stdin(args: &[Value]) -> InterpResult<Value> {
    let world = extract_world(args);
    Ok(io_ok(Value::nat_small(0), world)) // stdin fd = 0
}

fn io_handle_put_str(args: &[Value]) -> InterpResult<Value> {
    let world = extract_world(args);
    if let Some(s) = args.iter().find_map(|v| v.as_str()) {
        eprint!("{}", s);
    }
    Ok(io_ok(Value::unit(), world))
}

fn io_handle_put_str_ln(args: &[Value]) -> InterpResult<Value> {
    let world = extract_world(args);
    if let Some(s) = args.iter().find_map(|v| v.as_str()) {
        eprintln!("{}", s);
    }
    Ok(io_ok(Value::unit(), world))
}

fn io_handle_flush(args: &[Value]) -> InterpResult<Value> {
    let world = extract_world(args);
    Ok(io_ok(Value::unit(), world))
}

fn io_get_env(args: &[Value]) -> InterpResult<Value> {
    let world = extract_world(args);
    if let Some(key) = args.iter().find_map(|v| v.as_str()) {
        if let Ok(val) = std::env::var(key) {
            return Ok(io_ok(Value::some(Value::string(val.as_str())), world));
        }
    }
    Ok(io_ok(Value::none(), world))
}

fn io_is_eof(args: &[Value]) -> InterpResult<Value> {
    let world = extract_world(args);
    Ok(io_ok(Value::bool_(true), world)) // always EOF in non-interactive mode
}

fn io_get_line(args: &[Value]) -> InterpResult<Value> {
    let world = extract_world(args);
    Ok(io_ok(Value::string(""), world))
}

fn io_error_to_string(args: &[Value]) -> InterpResult<Value> {
    let _ = args;
    Ok(Value::string("<IO.Error>"))
}

fn io_error_user_error(args: &[Value]) -> InterpResult<Value> {
    let msg = args.iter().find_map(|v| v.as_str()).unwrap_or("<error>");
    Ok(Value::Ctor {
        tag: 0,
        name: Name::from_str_parts("IO.Error.userError"),
        fields: vec![Value::string(msg)],
    })
}

// --------------- Extra String builtins ---------------

fn string_to_nat_opt(args: &[Value]) -> InterpResult<Value> {
    let s = extract_str(args, 0).unwrap_or("");
    if let Ok(n) = s.parse::<BigUint>() {
        Ok(Value::some(Value::nat(n)))
    } else {
        Ok(Value::none())
    }
}

fn string_to_int_opt(args: &[Value]) -> InterpResult<Value> {
    let s = extract_str(args, 0).unwrap_or("");
    if let Ok(n) = s.parse::<BigInt>() {
        Ok(Value::some(Value::Int(Arc::new(n))))
    } else {
        Ok(Value::none())
    }
}

fn string_starts_with(args: &[Value]) -> InterpResult<Value> {
    let strs: Vec<&str> = args.iter().filter_map(|v| v.as_str()).collect();
    if strs.len() >= 2 {
        Ok(Value::bool_(strs[0].starts_with(strs[1])))
    } else {
        Ok(Value::bool_(false))
    }
}

fn string_ends_with(args: &[Value]) -> InterpResult<Value> {
    let strs: Vec<&str> = args.iter().filter_map(|v| v.as_str()).collect();
    if strs.len() >= 2 {
        Ok(Value::bool_(strs[0].ends_with(strs[1])))
    } else {
        Ok(Value::bool_(false))
    }
}

fn string_contains_char(args: &[Value]) -> InterpResult<Value> {
    // String.contains : String → Char → Bool
    let s = args.iter().find_map(|v| v.as_str()).unwrap_or("");
    let ch = args.iter().rev().find_map(|v| v.as_nat()).and_then(|n| {
        let code: u32 = n.try_into().ok()?;
        char::from_u32(code)
    });
    if let Some(c) = ch {
        Ok(Value::bool_(s.contains(c)))
    } else {
        Ok(Value::bool_(false))
    }
}

fn string_split_on(args: &[Value]) -> InterpResult<Value> {
    // String.splitOn : String → String → List String
    let strs: Vec<&str> = args.iter().filter_map(|v| v.as_str()).collect();
    let (s, sep) = if strs.len() >= 2 { (strs[0], strs[1]) } else { return Ok(Value::none()); };
    let parts: Vec<Value> = s.split(sep).map(Value::string).collect();
    let mut result = Value::Ctor { tag: 0, name: Name::from_str_parts("List.nil"), fields: vec![] };
    for part in parts.into_iter().rev() {
        result = Value::Ctor { tag: 1, name: Name::from_str_parts("List.cons"), fields: vec![part, result] };
    }
    Ok(result)
}

fn string_replace(args: &[Value]) -> InterpResult<Value> {
    let strs: Vec<&str> = args.iter().filter_map(|v| v.as_str()).collect();
    if strs.len() >= 3 {
        Ok(Value::string(strs[0].replace(strs[1], strs[2]).as_str()))
    } else {
        Ok(args.iter().find_map(|v| v.as_str()).map(Value::string).unwrap_or(Value::string("")))
    }
}

fn string_trim(args: &[Value]) -> InterpResult<Value> {
    let s = extract_str(args, 0).unwrap_or("");
    Ok(Value::string(s.trim()))
}

fn string_trim_left(args: &[Value]) -> InterpResult<Value> {
    let s = extract_str(args, 0).unwrap_or("");
    Ok(Value::string(s.trim_start()))
}

fn string_to_list(args: &[Value]) -> InterpResult<Value> {
    let s = extract_str(args, 0).unwrap_or("");
    let mut result = Value::Ctor { tag: 0, name: Name::from_str_parts("List.nil"), fields: vec![] };
    for ch in s.chars().rev() {
        result = Value::Ctor {
            tag: 1,
            name: Name::from_str_parts("List.cons"),
            fields: vec![Value::nat_small(ch as u64), result],
        };
    }
    Ok(result)
}

// --------------- Option helpers ---------------

fn option_is_some(args: &[Value]) -> InterpResult<Value> {
    let v = args.iter().find(|v| !matches!(v, Value::Erased)).unwrap_or(&Value::Erased);
    Ok(Value::bool_(matches!(v, Value::Ctor { tag: 1, .. })))
}

fn option_is_none(args: &[Value]) -> InterpResult<Value> {
    let v = args.iter().find(|v| !matches!(v, Value::Erased)).unwrap_or(&Value::Erased);
    Ok(Value::bool_(matches!(v, Value::Ctor { tag: 0, .. })))
}

fn option_get_bang(args: &[Value]) -> InterpResult<Value> {
    // Option.get! : {α : Type} → [Inhabited α] → Option α → α
    let v = args.iter().find(|v| matches!(v, Value::Ctor { tag: 1, .. }));
    match v {
        Some(Value::Ctor { fields, .. }) => Ok(fields.first().cloned().unwrap_or(Value::Erased)),
        _ => Err(InterpError::BuiltinError("Option.get!: called on None".into())),
    }
}
