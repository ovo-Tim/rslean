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
