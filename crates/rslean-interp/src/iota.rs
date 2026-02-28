use rslean_expr::ConstantInfo;
use rslean_level::Level;
use rslean_name::Name;

use crate::env::LocalEnv;
use crate::error::{InterpError, InterpResult};
use crate::eval::Interpreter;
use crate::value::Value;

/// Compute total arity of a recursor: params + motives + minors + indices + 1 (major premise).
pub fn recursor_total_arity(info: &ConstantInfo) -> u32 {
    match info {
        ConstantInfo::Recursor {
            num_params,
            num_motives,
            num_minors,
            num_indices,
            ..
        } => num_params + num_motives + num_minors + num_indices + 1,
        _ => 1,
    }
}

/// Apply a fully-applied recursor to perform iota reduction.
///
/// Args layout: [params..., motives..., minors..., indices..., major]
pub fn apply_recursor(
    interp: &mut Interpreter,
    rec_name: &Name,
    levels: &[Level],
    args: Vec<Value>,
) -> InterpResult<Value> {
    let info = interp.env().get(rec_name)?.clone();

    let (num_params, num_motives, num_minors, num_indices, rules) = match &info {
        ConstantInfo::Recursor {
            num_params,
            num_motives,
            num_minors,
            num_indices,
            rules,
            ..
        } => (
            *num_params as usize,
            *num_motives as usize,
            *num_minors as usize,
            *num_indices as usize,
            rules.clone(),
        ),
        _ => {
            return Err(InterpError::RecursorError(format!(
                "{} is not a recursor",
                rec_name
            )));
        }
    };

    let major_idx = num_params + num_motives + num_minors + num_indices;
    if args.len() <= major_idx {
        return Err(InterpError::RecursorError(format!(
            "recursor {} expected at least {} args, got {}",
            rec_name,
            major_idx + 1,
            args.len()
        )));
    }

    let major = &args[major_idx];

    // The major premise must be a constructor value.
    let (ctor_name, ctor_fields) = match major {
        Value::Ctor { name, fields, .. } => (name.clone(), fields.clone()),
        Value::Nat(n) => {
            // Nat is special: 0 → Nat.zero, succ n → Nat.succ(n)
            nat_to_ctor(n)
        }
        Value::Erased => return Ok(Value::Erased),
        _ => {
            return Err(InterpError::RecursorError(format!(
                "major premise of {} is not a constructor: {:?}",
                rec_name, major
            )));
        }
    };

    // Find the matching recursor rule.
    let rule = rules
        .iter()
        .find(|r| r.ctor_name == ctor_name)
        .ok_or_else(|| {
            InterpError::RecursorError(format!(
                "no recursor rule for constructor {} in {}",
                ctor_name, rec_name
            ))
        })?;

    // Build substitution for the rule RHS:
    // The RHS expects: params, motives, minors, then constructor fields
    // (with recursive arguments getting the recursive result applied).
    let mut subst: Vec<Value> = Vec::new();

    // 1. Parameters
    for arg in args.iter().take(num_params) {
        subst.push(arg.clone());
    }

    // 2. Motives
    for arg in args.iter().skip(num_params).take(num_motives) {
        subst.push(arg.clone());
    }

    // 3. Minors (case alternatives)
    for arg in args.iter().skip(num_params + num_motives).take(num_minors) {
        subst.push(arg.clone());
    }

    // 4. Constructor fields
    // For recursive fields, we need to also apply the recursor to them.
    // For now, just pass the fields directly — the rule RHS handles recursion
    // via the minor premises.
    for field in &ctor_fields {
        subst.push(field.clone());
    }

    // Instantiate the rule RHS with the substitution (reversed for de Bruijn).
    let level_params = info.level_params();
    let mut rhs = rule.rhs.clone();
    if !level_params.is_empty() && !levels.is_empty() {
        rhs = rhs.instantiate_level_params(level_params, levels);
    }

    // The RHS uses de Bruijn indices referring to the substitution in reverse order.
    let subst_exprs: Vec<rslean_expr::Expr> = (0..subst.len())
        .map(|_| rslean_expr::Expr::bvar(0)) // placeholder — we eval via local env
        .collect();
    let _ = subst_exprs;

    // The RHS has de Bruijn indices where bvar(0) = most recently bound.
    // Our subst is in order: [params, motives, minors, fields].
    // Since push() prepends, pushing in forward order means the last pushed
    // (fields) will be at bvar(0), which is the correct de Bruijn convention.
    let mut local_env = LocalEnv::new();
    for v in subst {
        local_env = local_env.push(v);
    }

    interp.eval(&rhs, &local_env)
}

/// Convert a Nat value to a constructor representation.
fn nat_to_ctor(n: &num_bigint::BigUint) -> (Name, Vec<Value>) {
    use num_traits::Zero;
    if n.is_zero() {
        (Name::from_str_parts("Nat.zero"), vec![])
    } else {
        let pred = n - 1u32;
        (
            Name::from_str_parts("Nat.succ"),
            vec![Value::Nat(std::sync::Arc::new(pred))],
        )
    }
}
