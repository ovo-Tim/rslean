use rslean_expr::Expr;
use rslean_name::Name;

/// Quotient type constant names.
pub fn quot_name() -> Name {
    Name::mk_simple("Quot")
}
pub fn quot_mk_name() -> Name {
    Name::from_str_parts("Quot.mk")
}
pub fn quot_lift_name() -> Name {
    Name::from_str_parts("Quot.lift")
}
pub fn quot_ind_name() -> Name {
    Name::from_str_parts("Quot.ind")
}

/// Check if a name is one of the quotient primitives.
pub fn is_quot_decl(n: &Name) -> bool {
    *n == quot_name() || *n == quot_mk_name() || *n == quot_lift_name() || *n == quot_ind_name()
}

/// Check if a name is a quotient recursor (lift or ind).
pub fn is_quot_rec(n: &Name) -> bool {
    *n == quot_lift_name() || *n == quot_ind_name()
}

/// Attempt to reduce a quotient recursor application.
///
/// `Quot.lift f h (Quot.mk a)` reduces to `f a`
/// `Quot.ind f h (Quot.mk a)` reduces to `f a`
pub fn quot_reduce_rec(e: &Expr, whnf: &dyn Fn(&Expr) -> Expr) -> Option<Expr> {
    if !e.is_app() {
        return None;
    }
    let f = e.get_app_fn();
    if !f.is_const() {
        return None;
    }
    let fn_name = f.const_name();
    if !is_quot_rec(fn_name) {
        return None;
    }

    let mut args = Vec::new();
    e.get_app_args(&mut args);

    // Quot.lift : {α : Sort u} → {β : Sort v} → {r : α → α → Prop}
    //            → (f : α → β) → ((a b : α) → r a b → f a = f b) → Quot r → β
    // Quot.ind : ...
    // The major premise (Quot r value) is at position 5 for lift, 4 for ind
    let major_idx = if *fn_name == quot_lift_name() { 5 } else { 4 };

    if args.len() <= major_idx {
        return None;
    }

    let major = whnf(&args[major_idx]);

    // Check if the major premise is `Quot.mk _ a`
    let major_fn = major.get_app_fn();
    if !major_fn.is_const() || *major_fn.const_name() != quot_mk_name() {
        return None;
    }

    let mut major_args = Vec::new();
    major.get_app_args(&mut major_args);

    // Quot.mk : {α : Sort u} → {r : α → α → Prop} → α → Quot r
    // The element `a` is at index 2
    if major_args.len() < 3 {
        return None;
    }

    let a = &major_args[2];

    // For Quot.lift: f is at index 3
    // For Quot.ind:  f is at index 3
    let f_idx = 3;
    if args.len() <= f_idx {
        return None;
    }

    let func = &args[f_idx];
    let mut result = Expr::app(func.clone(), a.clone());

    // Apply remaining arguments after major
    for arg in &args[major_idx + 1..] {
        result = Expr::app(result, arg.clone());
    }

    Some(result)
}
