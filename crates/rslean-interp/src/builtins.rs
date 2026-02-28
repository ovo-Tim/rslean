use num_bigint::BigUint;
use num_traits::{One, Zero};
use rslean_name::Name;
use rustc_hash::FxHashMap;
use std::cell::RefCell;
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

    // ST/Ref — mutable state
    reg(map, "ST.Ref.mk", st_ref_mk);
    reg(map, "ST.Ref.get", st_ref_get);
    reg(map, "ST.Ref.set", st_ref_set);
    reg(map, "ST.Ref.modifyGet", st_ref_modify_get);

    // IO stubs
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
}

fn reg(map: &mut FxHashMap<Name, BuiltinFn>, name: &str, f: BuiltinFn) {
    map.insert(Name::from_str_parts(name), f);
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

// --------------- ST/Ref builtins ---------------

fn st_ref_mk(args: &[Value]) -> InterpResult<Value> {
    // ST.Ref.mk : {σ : Type} → {α : Type} → α → ST σ (Ref σ α)
    // Find the non-Erased value to store in the ref
    let initial = args
        .iter()
        .rev()
        .find(|v| !matches!(v, Value::Erased))
        .cloned()
        .unwrap_or(Value::Erased);
    Ok(Value::Ref(Arc::new(RefCell::new(initial))))
}

fn st_ref_get(args: &[Value]) -> InterpResult<Value> {
    // ST.Ref.get : {σ : Type} → {α : Type} → Ref σ α → ST σ α
    let r = find_ref(args)?;
    Ok(r.borrow().clone())
}

fn st_ref_set(args: &[Value]) -> InterpResult<Value> {
    // ST.Ref.set : {σ : Type} → {α : Type} → Ref σ α → α → ST σ PUnit
    let r = find_ref(args)?;
    let val = args
        .iter()
        .rev()
        .find(|v| !matches!(v, Value::Erased | Value::Ref(_)))
        .cloned()
        .unwrap_or(Value::Erased);
    *r.borrow_mut() = val;
    Ok(Value::unit())
}

fn st_ref_modify_get(args: &[Value]) -> InterpResult<Value> {
    // ST.Ref.modifyGet : {σ α β : Type} → Ref σ α → (α → β × α) → ST σ β
    // This is complex — the second arg is a function. For now, stub it.
    let _ = args;
    Ok(Value::Erased)
}

fn find_ref(args: &[Value]) -> InterpResult<&RefCell<Value>> {
    args.iter()
        .find_map(|v| match v {
            Value::Ref(r) => Some(r.as_ref()),
            _ => None,
        })
        .ok_or_else(|| InterpError::BuiltinError("expected Ref arg".into()))
}

// --------------- IO stubs ---------------

fn io_println(args: &[Value]) -> InterpResult<Value> {
    if let Some(s) = args.iter().find_map(|v| v.as_str()) {
        eprintln!("[IO.println] {}", s);
    }
    // Return EStateM.Result.ok PUnit.unit ()
    Ok(Value::unit())
}

fn io_print(args: &[Value]) -> InterpResult<Value> {
    if let Some(s) = args.iter().find_map(|v| v.as_str()) {
        eprint!("[IO.print] {}", s);
    }
    Ok(Value::unit())
}

fn io_eprintln(args: &[Value]) -> InterpResult<Value> {
    if let Some(s) = args.iter().find_map(|v| v.as_str()) {
        eprintln!("[IO.eprintln] {}", s);
    }
    Ok(Value::unit())
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
