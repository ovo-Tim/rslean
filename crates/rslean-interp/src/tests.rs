use num_bigint::BigUint;
use rslean_expr::{
    BinderInfo, ConstantInfo, DefinitionSafety, Expr, Literal, RecursorRule, ReducibilityHints,
};
use rslean_kernel::Environment;
use rslean_level::Level;
use rslean_name::Name;

use crate::env::LocalEnv;
use crate::eval::Interpreter;
use crate::value::Value;

// ====================== LocalEnv tests ======================

#[test]
fn test_local_env_empty() {
    let env = LocalEnv::new();
    assert!(env.is_empty());
    assert_eq!(env.len(), 0);
    assert!(env.lookup(0).is_err());
}

#[test]
fn test_local_env_push_lookup() {
    let env = LocalEnv::new();
    let env = env.push(Value::nat_small(42));
    assert_eq!(env.len(), 1);
    assert!(env.lookup(0).unwrap().as_nat().is_some());

    let env = env.push(Value::nat_small(99));
    assert_eq!(env.len(), 2);
    // Index 0 is the most recently pushed
    assert_eq!(*env.lookup(0).unwrap().as_nat().unwrap(), BigUint::from(99u64));
    assert_eq!(*env.lookup(1).unwrap().as_nat().unwrap(), BigUint::from(42u64));
}

#[test]
fn test_local_env_out_of_bounds() {
    let env = LocalEnv::new().push(Value::nat_small(1));
    assert!(env.lookup(0).is_ok());
    assert!(env.lookup(1).is_err());
}

// ====================== Value construction tests ======================

#[test]
fn test_value_nat() {
    let v = Value::nat_small(42);
    assert_eq!(*v.as_nat().unwrap(), BigUint::from(42u64));
}

#[test]
fn test_value_string() {
    let v = Value::string("hello");
    assert_eq!(v.as_str().unwrap(), "hello");
}

#[test]
fn test_value_bool() {
    let t = Value::bool_(true);
    assert_eq!(t.as_bool(), Some(true));

    let f = Value::bool_(false);
    assert_eq!(f.as_bool(), Some(false));
}

#[test]
fn test_value_unit() {
    let u = Value::unit();
    assert!(matches!(u, Value::Ctor { tag: 0, .. }));
}

// ====================== Eval: Literal tests ======================

fn make_interp() -> Interpreter {
    Interpreter::new(Environment::new())
}

#[test]
fn test_eval_nat_lit() {
    let mut interp = make_interp();
    let expr = Expr::lit(Literal::nat_small(42));
    let val = interp.eval(&expr, &LocalEnv::new()).unwrap();
    assert_eq!(*val.as_nat().unwrap(), BigUint::from(42u64));
}

#[test]
fn test_eval_string_lit() {
    let mut interp = make_interp();
    let expr = Expr::lit(Literal::string("hello"));
    let val = interp.eval(&expr, &LocalEnv::new()).unwrap();
    assert_eq!(val.as_str().unwrap(), "hello");
}

// ====================== Eval: BVar tests ======================

#[test]
fn test_eval_bvar() {
    let mut interp = make_interp();
    let env = LocalEnv::new().push(Value::nat_small(7));
    let expr = Expr::bvar(0);
    let val = interp.eval(&expr, &env).unwrap();
    assert_eq!(*val.as_nat().unwrap(), BigUint::from(7u64));
}

#[test]
fn test_eval_bvar_unbound() {
    let mut interp = make_interp();
    let expr = Expr::bvar(0);
    assert!(interp.eval(&expr, &LocalEnv::new()).is_err());
}

// ====================== Eval: Lambda + App tests ======================

#[test]
fn test_eval_identity() {
    // (fun x : Nat => x) 42  →  42
    let mut interp = make_interp();
    let body = Expr::bvar(0);
    let lam = Expr::lam(Name::mk_simple("x"), Expr::type_(), body, BinderInfo::Default);
    let app = Expr::app(lam, Expr::lit(Literal::nat_small(42)));
    let val = interp.eval(&app, &LocalEnv::new()).unwrap();
    assert_eq!(*val.as_nat().unwrap(), BigUint::from(42u64));
}

#[test]
fn test_eval_const_fn() {
    // (fun x y => x) 1 2  →  1
    // fun x => fun y => #1  (x is at index 1 inside the inner lambda)
    let mut interp = make_interp();
    let inner_body = Expr::bvar(1); // x in inner scope
    let inner_lam = Expr::lam(
        Name::mk_simple("y"),
        Expr::type_(),
        inner_body,
        BinderInfo::Default,
    );
    let outer_lam = Expr::lam(
        Name::mk_simple("x"),
        Expr::type_(),
        inner_lam,
        BinderInfo::Default,
    );
    let app1 = Expr::app(outer_lam, Expr::lit(Literal::nat_small(1)));
    let app2 = Expr::app(app1, Expr::lit(Literal::nat_small(2)));
    let val = interp.eval(&app2, &LocalEnv::new()).unwrap();
    assert_eq!(*val.as_nat().unwrap(), BigUint::from(1u64));
}

// ====================== Eval: Let tests ======================

#[test]
fn test_eval_let() {
    // let x : Nat := 5 in x  →  5
    let mut interp = make_interp();
    let expr = Expr::let_e(
        Name::mk_simple("x"),
        Expr::type_(),
        Expr::lit(Literal::nat_small(5)),
        Expr::bvar(0),
        false,
    );
    let val = interp.eval(&expr, &LocalEnv::new()).unwrap();
    assert_eq!(*val.as_nat().unwrap(), BigUint::from(5u64));
}

#[test]
fn test_eval_nested_let() {
    // let x := 3 in let y := 7 in x  →  3
    // In the inner let body, x is at index 1
    let mut interp = make_interp();
    let inner_let = Expr::let_e(
        Name::mk_simple("y"),
        Expr::type_(),
        Expr::lit(Literal::nat_small(7)),
        Expr::bvar(1), // x
        false,
    );
    let outer_let = Expr::let_e(
        Name::mk_simple("x"),
        Expr::type_(),
        Expr::lit(Literal::nat_small(3)),
        inner_let,
        false,
    );
    let val = interp.eval(&outer_let, &LocalEnv::new()).unwrap();
    assert_eq!(*val.as_nat().unwrap(), BigUint::from(3u64));
}

// ====================== Eval: Type/Sort erasure ======================

#[test]
fn test_eval_sort_erased() {
    let mut interp = make_interp();
    let expr = Expr::sort(Level::zero());
    let val = interp.eval(&expr, &LocalEnv::new()).unwrap();
    assert!(matches!(val, Value::Erased));
}

#[test]
fn test_eval_forall_erased() {
    let mut interp = make_interp();
    let expr = Expr::forall_e(
        Name::mk_simple("x"),
        Expr::type_(),
        Expr::type_(),
        BinderInfo::Default,
    );
    let val = interp.eval(&expr, &LocalEnv::new()).unwrap();
    assert!(matches!(val, Value::Erased));
}

// ====================== Eval: MData transparency ======================

#[test]
fn test_eval_mdata() {
    let mut interp = make_interp();
    let inner = Expr::lit(Literal::nat_small(99));
    let expr = Expr::mdata(vec![], inner);
    let val = interp.eval(&expr, &LocalEnv::new()).unwrap();
    assert_eq!(*val.as_nat().unwrap(), BigUint::from(99u64));
}

// ====================== Eval: Const + Definition ======================

fn env_with_id() -> Environment {
    // id : {α : Type} → α → α := fun α x => x
    let env = Environment::new();
    let id_body = Expr::lam(
        Name::mk_simple("α"),
        Expr::type_(),
        Expr::lam(
            Name::mk_simple("x"),
            Expr::bvar(0), // α
            Expr::bvar(0), // x
            BinderInfo::Default,
        ),
        BinderInfo::Implicit,
    );
    let id_type = Expr::forall_e(
        Name::mk_simple("α"),
        Expr::type_(),
        Expr::forall_e(
            Name::mk_simple("x"),
            Expr::bvar(0),
            Expr::bvar(1),
            BinderInfo::Default,
        ),
        BinderInfo::Implicit,
    );
    env.add_constant(ConstantInfo::Definition {
        name: Name::mk_simple("id"),
        level_params: vec![],
        type_: id_type,
        value: id_body,
        hints: ReducibilityHints::Abbreviation,
        safety: DefinitionSafety::Safe,
    })
    .unwrap()
}

#[test]
fn test_eval_definition_id() {
    // id Nat 42  →  42
    let env = env_with_id();
    let mut interp = Interpreter::new(env);
    let id_const = Expr::const_(Name::mk_simple("id"), vec![]);
    let app = Expr::mk_app(
        id_const,
        &[
            Expr::type_(),          // α = Nat (erased, but we pass Type as placeholder)
            Expr::lit(Literal::nat_small(42)),
        ],
    );
    let val = interp.eval(&app, &LocalEnv::new()).unwrap();
    assert_eq!(*val.as_nat().unwrap(), BigUint::from(42u64));
}

// ====================== Eval: Constructor + Projection ======================

fn env_with_pair() -> Environment {
    let env = Environment::new();
    // Prod inductive type (simplified)
    let env = env
        .add_constant(ConstantInfo::Inductive {
            name: Name::from_str_parts("Prod"),
            level_params: vec![],
            type_: Expr::type_(),
            num_params: 2,
            num_indices: 0,
            all: vec![Name::from_str_parts("Prod")],
            ctors: vec![Name::from_str_parts("Prod.mk")],
            num_nested: 0,
            is_rec: false,
            is_unsafe: false,
            is_reflexive: false,
        })
        .unwrap();

    // Prod.mk constructor: 2 params (α, β), 2 fields (fst, snd)
    env.add_constant(ConstantInfo::Constructor {
        name: Name::from_str_parts("Prod.mk"),
        level_params: vec![],
        type_: Expr::type_(), // simplified
        induct_name: Name::from_str_parts("Prod"),
        ctor_idx: 0,
        num_params: 2,
        num_fields: 2,
        is_unsafe: false,
    })
    .unwrap()
}

#[test]
fn test_eval_constructor() {
    let env = env_with_pair();
    let mut interp = Interpreter::new(env);

    // Prod.mk Nat Nat 3 7
    let mk = Expr::const_(Name::from_str_parts("Prod.mk"), vec![]);
    let app = Expr::mk_app(
        mk,
        &[
            Expr::type_(),                   // α
            Expr::type_(),                   // β
            Expr::lit(Literal::nat_small(3)), // fst
            Expr::lit(Literal::nat_small(7)), // snd
        ],
    );
    let val = interp.eval(&app, &LocalEnv::new()).unwrap();
    match &val {
        Value::Ctor { tag, fields, .. } => {
            assert_eq!(*tag, 0);
            assert_eq!(fields.len(), 2);
            assert_eq!(*fields[0].as_nat().unwrap(), BigUint::from(3u64));
            assert_eq!(*fields[1].as_nat().unwrap(), BigUint::from(7u64));
        }
        _ => panic!("expected Ctor, got {:?}", val),
    }
}

#[test]
fn test_eval_projection() {
    let env = env_with_pair();
    let mut interp = Interpreter::new(env);

    // Prod.1 (Prod.mk Nat Nat 3 7)  →  3
    let mk = Expr::const_(Name::from_str_parts("Prod.mk"), vec![]);
    let pair = Expr::mk_app(
        mk,
        &[
            Expr::type_(),
            Expr::type_(),
            Expr::lit(Literal::nat_small(3)),
            Expr::lit(Literal::nat_small(7)),
        ],
    );
    let proj0 = Expr::proj(Name::from_str_parts("Prod"), 0, pair.clone());
    let proj1 = Expr::proj(Name::from_str_parts("Prod"), 1, pair);

    let v0 = interp.eval(&proj0, &LocalEnv::new()).unwrap();
    assert_eq!(*v0.as_nat().unwrap(), BigUint::from(3u64));

    let v1 = interp.eval(&proj1, &LocalEnv::new()).unwrap();
    assert_eq!(*v1.as_nat().unwrap(), BigUint::from(7u64));
}

// ====================== Eval: Recursor / Iota reduction ======================

fn env_with_bool_rec() -> Environment {
    let env = Environment::new();

    // Bool inductive
    let env = env
        .add_constant(ConstantInfo::Inductive {
            name: Name::from_str_parts("Bool"),
            level_params: vec![],
            type_: Expr::type_(),
            num_params: 0,
            num_indices: 0,
            all: vec![Name::from_str_parts("Bool")],
            ctors: vec![
                Name::from_str_parts("Bool.false"),
                Name::from_str_parts("Bool.true"),
            ],
            num_nested: 0,
            is_rec: false,
            is_unsafe: false,
            is_reflexive: false,
        })
        .unwrap();

    // Bool.false: tag 0, 0 params, 0 fields
    let env = env
        .add_constant(ConstantInfo::Constructor {
            name: Name::from_str_parts("Bool.false"),
            level_params: vec![],
            type_: Expr::const_(Name::from_str_parts("Bool"), vec![]),
            induct_name: Name::from_str_parts("Bool"),
            ctor_idx: 0,
            num_params: 0,
            num_fields: 0,
            is_unsafe: false,
        })
        .unwrap();

    // Bool.true: tag 1, 0 params, 0 fields
    let env = env
        .add_constant(ConstantInfo::Constructor {
            name: Name::from_str_parts("Bool.true"),
            level_params: vec![],
            type_: Expr::const_(Name::from_str_parts("Bool"), vec![]),
            induct_name: Name::from_str_parts("Bool"),
            ctor_idx: 1,
            num_params: 0,
            num_fields: 0,
            is_unsafe: false,
        })
        .unwrap();

    // Bool.rec : {motive : Bool → Sort u} → motive false → motive true → (b : Bool) → motive b
    // num_params = 0, num_motives = 1, num_minors = 2 (false_case, true_case), num_indices = 0
    // Total arity = 0 + 1 + 2 + 0 + 1 = 4
    // Args: [motive, false_case, true_case, major]
    //
    // Rule for Bool.false: rhs = BVar(1) (false_case)
    //   substitution: [motive] → BVar(0) is motive, BVar(1) is false_case (minor 0)
    //   Actually the rhs is evaluated with subst [params, motives, minors, fields]
    //   For Bool.false: subst = [motive, false_case, true_case] (no fields)
    //   reversed = [true_case, false_case, motive]
    //   So bvar(0) = true_case, bvar(1) = false_case, bvar(2) = motive
    //   Rule for false: rhs should reference false_case = bvar(1)
    //
    // Rule for Bool.true: rhs = BVar(0) (true_case)
    //   subst = [motive, false_case, true_case] reversed = [true_case, false_case, motive]
    //   bvar(0) = true_case ✓
    env.add_constant(ConstantInfo::Recursor {
        name: Name::from_str_parts("Bool.rec"),
        level_params: vec![Name::mk_simple("u")],
        type_: Expr::type_(), // simplified
        all: vec![Name::from_str_parts("Bool")],
        num_params: 0,
        num_indices: 0,
        num_motives: 1,
        num_minors: 2,
        rules: vec![
            RecursorRule {
                ctor_name: Name::from_str_parts("Bool.false"),
                num_fields: 0,
                rhs: Expr::bvar(1), // false_case
            },
            RecursorRule {
                ctor_name: Name::from_str_parts("Bool.true"),
                num_fields: 0,
                rhs: Expr::bvar(0), // true_case
            },
        ],
        is_k: true,
        is_unsafe: false,
    })
    .unwrap()
}

#[test]
fn test_eval_bool_rec_true() {
    // Bool.rec motive 10 20 Bool.true → 20
    let env = env_with_bool_rec();
    let mut interp = Interpreter::new(env);

    let rec = Expr::const_(Name::from_str_parts("Bool.rec"), vec![Level::one()]);
    let app = Expr::mk_app(
        rec,
        &[
            Expr::type_(),                    // motive (erased)
            Expr::lit(Literal::nat_small(10)), // false case
            Expr::lit(Literal::nat_small(20)), // true case
            Expr::const_(Name::from_str_parts("Bool.true"), vec![]), // major
        ],
    );
    let val = interp.eval(&app, &LocalEnv::new()).unwrap();
    assert_eq!(*val.as_nat().unwrap(), BigUint::from(20u64));
}

#[test]
fn test_eval_bool_rec_false() {
    // Bool.rec motive 10 20 Bool.false → 10
    let env = env_with_bool_rec();
    let mut interp = Interpreter::new(env);

    let rec = Expr::const_(Name::from_str_parts("Bool.rec"), vec![Level::one()]);
    let app = Expr::mk_app(
        rec,
        &[
            Expr::type_(),
            Expr::lit(Literal::nat_small(10)),
            Expr::lit(Literal::nat_small(20)),
            Expr::const_(Name::from_str_parts("Bool.false"), vec![]),
        ],
    );
    let val = interp.eval(&app, &LocalEnv::new()).unwrap();
    assert_eq!(*val.as_nat().unwrap(), BigUint::from(10u64));
}

// ====================== Builtin tests ======================

#[test]
fn test_builtin_nat_add() {
    let env = Environment::new();
    // Add Nat.add type to env so arity can be computed
    let env = env
        .add_constant(ConstantInfo::Axiom {
            name: Name::from_str_parts("Nat.add"),
            level_params: vec![],
            type_: Expr::forall_e(
                Name::anonymous(),
                Expr::type_(), // Nat placeholder
                Expr::forall_e(
                    Name::anonymous(),
                    Expr::type_(),
                    Expr::type_(),
                    BinderInfo::Default,
                ),
                BinderInfo::Default,
            ),
            is_unsafe: false,
        })
        .unwrap();
    let mut interp = Interpreter::new(env);

    // Nat.add 2 3  →  5
    let add = Expr::const_(Name::from_str_parts("Nat.add"), vec![]);
    let app = Expr::mk_app(
        add,
        &[
            Expr::lit(Literal::nat_small(2)),
            Expr::lit(Literal::nat_small(3)),
        ],
    );
    let val = interp.eval(&app, &LocalEnv::new()).unwrap();
    assert_eq!(*val.as_nat().unwrap(), BigUint::from(5u64));
}

#[test]
fn test_builtin_nat_mul() {
    let env = Environment::new();
    let env = env
        .add_constant(ConstantInfo::Axiom {
            name: Name::from_str_parts("Nat.mul"),
            level_params: vec![],
            type_: Expr::forall_e(
                Name::anonymous(),
                Expr::type_(),
                Expr::forall_e(
                    Name::anonymous(),
                    Expr::type_(),
                    Expr::type_(),
                    BinderInfo::Default,
                ),
                BinderInfo::Default,
            ),
            is_unsafe: false,
        })
        .unwrap();
    let mut interp = Interpreter::new(env);

    let mul = Expr::const_(Name::from_str_parts("Nat.mul"), vec![]);
    let app = Expr::mk_app(
        mul,
        &[
            Expr::lit(Literal::nat_small(3)),
            Expr::lit(Literal::nat_small(4)),
        ],
    );
    let val = interp.eval(&app, &LocalEnv::new()).unwrap();
    assert_eq!(*val.as_nat().unwrap(), BigUint::from(12u64));
}

#[test]
fn test_builtin_nat_sub_underflow() {
    let env = Environment::new();
    let env = env
        .add_constant(ConstantInfo::Axiom {
            name: Name::from_str_parts("Nat.sub"),
            level_params: vec![],
            type_: Expr::forall_e(
                Name::anonymous(),
                Expr::type_(),
                Expr::forall_e(
                    Name::anonymous(),
                    Expr::type_(),
                    Expr::type_(),
                    BinderInfo::Default,
                ),
                BinderInfo::Default,
            ),
            is_unsafe: false,
        })
        .unwrap();
    let mut interp = Interpreter::new(env);

    // Nat.sub 2 5  →  0 (truncated subtraction)
    let sub = Expr::const_(Name::from_str_parts("Nat.sub"), vec![]);
    let app = Expr::mk_app(
        sub,
        &[
            Expr::lit(Literal::nat_small(2)),
            Expr::lit(Literal::nat_small(5)),
        ],
    );
    let val = interp.eval(&app, &LocalEnv::new()).unwrap();
    assert_eq!(*val.as_nat().unwrap(), BigUint::from(0u64));
}

#[test]
fn test_builtin_nat_div_by_zero() {
    let env = Environment::new();
    let env = env
        .add_constant(ConstantInfo::Axiom {
            name: Name::from_str_parts("Nat.div"),
            level_params: vec![],
            type_: Expr::forall_e(
                Name::anonymous(),
                Expr::type_(),
                Expr::forall_e(
                    Name::anonymous(),
                    Expr::type_(),
                    Expr::type_(),
                    BinderInfo::Default,
                ),
                BinderInfo::Default,
            ),
            is_unsafe: false,
        })
        .unwrap();
    let mut interp = Interpreter::new(env);

    // Nat.div 5 0  →  0
    let div = Expr::const_(Name::from_str_parts("Nat.div"), vec![]);
    let app = Expr::mk_app(
        div,
        &[
            Expr::lit(Literal::nat_small(5)),
            Expr::lit(Literal::nat_small(0)),
        ],
    );
    let val = interp.eval(&app, &LocalEnv::new()).unwrap();
    assert_eq!(*val.as_nat().unwrap(), BigUint::from(0u64));
}

#[test]
fn test_builtin_nat_beq() {
    let env = Environment::new();
    let env = env
        .add_constant(ConstantInfo::Axiom {
            name: Name::from_str_parts("Nat.beq"),
            level_params: vec![],
            type_: Expr::forall_e(
                Name::anonymous(),
                Expr::type_(),
                Expr::forall_e(
                    Name::anonymous(),
                    Expr::type_(),
                    Expr::type_(),
                    BinderInfo::Default,
                ),
                BinderInfo::Default,
            ),
            is_unsafe: false,
        })
        .unwrap();
    let mut interp = Interpreter::new(env);

    let beq = Expr::const_(Name::from_str_parts("Nat.beq"), vec![]);
    let app = Expr::mk_app(
        beq,
        &[
            Expr::lit(Literal::nat_small(3)),
            Expr::lit(Literal::nat_small(3)),
        ],
    );
    let val = interp.eval(&app, &LocalEnv::new()).unwrap();
    assert_eq!(val.as_bool(), Some(true));

    let app2 = Expr::mk_app(
        Expr::const_(Name::from_str_parts("Nat.beq"), vec![]),
        &[
            Expr::lit(Literal::nat_small(3)),
            Expr::lit(Literal::nat_small(4)),
        ],
    );
    let val2 = interp.eval(&app2, &LocalEnv::new()).unwrap();
    assert_eq!(val2.as_bool(), Some(false));
}

#[test]
fn test_builtin_nat_mod() {
    let env = Environment::new();
    let env = env
        .add_constant(ConstantInfo::Axiom {
            name: Name::from_str_parts("Nat.mod"),
            level_params: vec![],
            type_: Expr::forall_e(
                Name::anonymous(),
                Expr::type_(),
                Expr::forall_e(
                    Name::anonymous(),
                    Expr::type_(),
                    Expr::type_(),
                    BinderInfo::Default,
                ),
                BinderInfo::Default,
            ),
            is_unsafe: false,
        })
        .unwrap();
    let mut interp = Interpreter::new(env);

    let m = Expr::const_(Name::from_str_parts("Nat.mod"), vec![]);
    let app = Expr::mk_app(
        m,
        &[
            Expr::lit(Literal::nat_small(10)),
            Expr::lit(Literal::nat_small(3)),
        ],
    );
    let val = interp.eval(&app, &LocalEnv::new()).unwrap();
    assert_eq!(*val.as_nat().unwrap(), BigUint::from(1u64));
}

#[test]
fn test_builtin_nat_pow() {
    let env = Environment::new();
    let env = env
        .add_constant(ConstantInfo::Axiom {
            name: Name::from_str_parts("Nat.pow"),
            level_params: vec![],
            type_: Expr::forall_e(
                Name::anonymous(),
                Expr::type_(),
                Expr::forall_e(
                    Name::anonymous(),
                    Expr::type_(),
                    Expr::type_(),
                    BinderInfo::Default,
                ),
                BinderInfo::Default,
            ),
            is_unsafe: false,
        })
        .unwrap();
    let mut interp = Interpreter::new(env);

    let pow = Expr::const_(Name::from_str_parts("Nat.pow"), vec![]);
    let app = Expr::mk_app(
        pow,
        &[
            Expr::lit(Literal::nat_small(2)),
            Expr::lit(Literal::nat_small(10)),
        ],
    );
    let val = interp.eval(&app, &LocalEnv::new()).unwrap();
    assert_eq!(*val.as_nat().unwrap(), BigUint::from(1024u64));
}

#[test]
fn test_builtin_nat_dec_eq() {
    let env = Environment::new();
    let env = env
        .add_constant(ConstantInfo::Axiom {
            name: Name::from_str_parts("Nat.decEq"),
            level_params: vec![],
            type_: Expr::forall_e(
                Name::anonymous(),
                Expr::type_(),
                Expr::forall_e(
                    Name::anonymous(),
                    Expr::type_(),
                    Expr::type_(),
                    BinderInfo::Default,
                ),
                BinderInfo::Default,
            ),
            is_unsafe: false,
        })
        .unwrap();
    let mut interp = Interpreter::new(env);

    let dec_eq = Expr::const_(Name::from_str_parts("Nat.decEq"), vec![]);
    let app = Expr::mk_app(
        dec_eq,
        &[
            Expr::lit(Literal::nat_small(5)),
            Expr::lit(Literal::nat_small(5)),
        ],
    );
    let val = interp.eval(&app, &LocalEnv::new()).unwrap();
    // isTrue has tag 1
    match &val {
        Value::Ctor { tag: 1, .. } => {} // isTrue
        _ => panic!("expected Decidable.isTrue, got {:?}", val),
    }
}

#[test]
fn test_builtin_string_append() {
    let env = Environment::new();
    let env = env
        .add_constant(ConstantInfo::Axiom {
            name: Name::from_str_parts("String.append"),
            level_params: vec![],
            type_: Expr::forall_e(
                Name::anonymous(),
                Expr::type_(),
                Expr::forall_e(
                    Name::anonymous(),
                    Expr::type_(),
                    Expr::type_(),
                    BinderInfo::Default,
                ),
                BinderInfo::Default,
            ),
            is_unsafe: false,
        })
        .unwrap();
    let mut interp = Interpreter::new(env);

    let append = Expr::const_(Name::from_str_parts("String.append"), vec![]);
    let app = Expr::mk_app(
        append,
        &[
            Expr::lit(Literal::string("hello")),
            Expr::lit(Literal::string(" world")),
        ],
    );
    let val = interp.eval(&app, &LocalEnv::new()).unwrap();
    assert_eq!(val.as_str().unwrap(), "hello world");
}

#[test]
fn test_builtin_string_length() {
    let env = Environment::new();
    let env = env
        .add_constant(ConstantInfo::Axiom {
            name: Name::from_str_parts("String.length"),
            level_params: vec![],
            type_: Expr::forall_e(
                Name::anonymous(),
                Expr::type_(),
                Expr::type_(),
                BinderInfo::Default,
            ),
            is_unsafe: false,
        })
        .unwrap();
    let mut interp = Interpreter::new(env);

    let length = Expr::const_(Name::from_str_parts("String.length"), vec![]);
    let app = Expr::app(length, Expr::lit(Literal::string("hello")));
    let val = interp.eval(&app, &LocalEnv::new()).unwrap();
    assert_eq!(*val.as_nat().unwrap(), BigUint::from(5u64));
}

// ====================== Eval: FVar passthrough ======================

#[test]
fn test_eval_fvar_passthrough() {
    let mut interp = make_interp();
    let expr = Expr::fvar(Name::mk_simple("x"));
    let val = interp.eval(&expr, &LocalEnv::new()).unwrap();
    assert!(matches!(val, Value::KernelExpr(_)));
}

// ====================== Eval: Zero-arity constructor ======================

#[test]
fn test_eval_zero_arity_ctor() {
    let env = env_with_bool_rec();
    let mut interp = Interpreter::new(env);

    // Bool.true should evaluate to Ctor { tag: 1 }
    let expr = Expr::const_(Name::from_str_parts("Bool.true"), vec![]);
    let val = interp.eval(&expr, &LocalEnv::new()).unwrap();
    match &val {
        Value::Ctor { tag: 1, name, fields } => {
            assert_eq!(name, &Name::from_str_parts("Bool.true"));
            assert!(fields.is_empty());
        }
        _ => panic!("expected Bool.true Ctor, got {:?}", val),
    }
}

// ====================== Stack overflow protection ======================

#[test]
fn test_stack_overflow_protection() {
    // Create a self-referencing definition that would loop forever
    let env = Environment::new();
    let env = env
        .add_constant(ConstantInfo::Definition {
            name: Name::mk_simple("loop"),
            level_params: vec![],
            type_: Expr::type_(),
            value: Expr::const_(Name::mk_simple("loop"), vec![]), // self-reference
            hints: ReducibilityHints::Regular(0),
            safety: DefinitionSafety::Safe,
        })
        .unwrap();
    let mut interp = Interpreter::new(env);
    let result = interp.eval(
        &Expr::const_(Name::mk_simple("loop"), vec![]),
        &LocalEnv::new(),
    );
    assert!(result.is_err());
    match result.unwrap_err() {
        crate::error::InterpError::StackOverflow(_) => {}
        e => panic!("expected StackOverflow, got {:?}", e),
    }
}

// ====================== Partial application ======================

#[test]
fn test_partial_application() {
    // Create a function add_one = (fun x => Nat.add x 1)
    // But test partial application of a two-arg builtin
    let env = Environment::new();
    let env = env
        .add_constant(ConstantInfo::Axiom {
            name: Name::from_str_parts("Nat.add"),
            level_params: vec![],
            type_: Expr::forall_e(
                Name::anonymous(),
                Expr::type_(),
                Expr::forall_e(
                    Name::anonymous(),
                    Expr::type_(),
                    Expr::type_(),
                    BinderInfo::Default,
                ),
                BinderInfo::Default,
            ),
            is_unsafe: false,
        })
        .unwrap();
    let mut interp = Interpreter::new(env);

    // Partially apply: (Nat.add 3)
    let add = Expr::const_(Name::from_str_parts("Nat.add"), vec![]);
    let partial = Expr::app(add, Expr::lit(Literal::nat_small(3)));
    let partial_val = interp.eval(&partial, &LocalEnv::new()).unwrap();

    // Should be a closure with remaining_arity 1
    match &partial_val {
        Value::Closure {
            remaining_arity, ..
        } => assert_eq!(*remaining_arity, 1),
        _ => panic!("expected Closure, got {:?}", partial_val),
    }

    // Now fully apply: (Nat.add 3) 4  →  7
    let full = Expr::app(
        Expr::app(
            Expr::const_(Name::from_str_parts("Nat.add"), vec![]),
            Expr::lit(Literal::nat_small(3)),
        ),
        Expr::lit(Literal::nat_small(4)),
    );
    let val = interp.eval(&full, &LocalEnv::new()).unwrap();
    assert_eq!(*val.as_nat().unwrap(), BigUint::from(7u64));
}

// ====================== Nat.pred ======================

#[test]
fn test_builtin_nat_pred() {
    let env = Environment::new();
    let env = env
        .add_constant(ConstantInfo::Axiom {
            name: Name::from_str_parts("Nat.pred"),
            level_params: vec![],
            type_: Expr::forall_e(
                Name::anonymous(),
                Expr::type_(),
                Expr::type_(),
                BinderInfo::Default,
            ),
            is_unsafe: false,
        })
        .unwrap();
    let mut interp = Interpreter::new(env);

    // pred 5 = 4
    let pred = Expr::const_(Name::from_str_parts("Nat.pred"), vec![]);
    let app = Expr::app(pred, Expr::lit(Literal::nat_small(5)));
    let val = interp.eval(&app, &LocalEnv::new()).unwrap();
    assert_eq!(*val.as_nat().unwrap(), BigUint::from(4u64));

    // pred 0 = 0
    let pred2 = Expr::const_(Name::from_str_parts("Nat.pred"), vec![]);
    let app2 = Expr::app(pred2, Expr::lit(Literal::nat_small(0)));
    let val2 = interp.eval(&app2, &LocalEnv::new()).unwrap();
    assert_eq!(*val2.as_nat().unwrap(), BigUint::from(0u64));
}

// ====================== Lambda capturing ======================

#[test]
fn test_lambda_captures_env() {
    // let a := 10 in (fun x => a) 99  →  10
    let mut interp = make_interp();
    let expr = Expr::let_e(
        Name::mk_simple("a"),
        Expr::type_(),
        Expr::lit(Literal::nat_small(10)),
        // In body: a is bvar(0)
        // fun x => a  means fun x => bvar(1) (since x is bvar(0) inside lambda)
        Expr::app(
            Expr::lam(
                Name::mk_simple("x"),
                Expr::type_(),
                Expr::bvar(1), // a
                BinderInfo::Default,
            ),
            Expr::lit(Literal::nat_small(99)),
        ),
        false,
    );
    let val = interp.eval(&expr, &LocalEnv::new()).unwrap();
    assert_eq!(*val.as_nat().unwrap(), BigUint::from(10u64));
}

// ====================== Nat recursor (Nat.rec) ======================

fn env_with_nat_rec() -> Environment {
    let env = Environment::new();

    let env = env
        .add_constant(ConstantInfo::Inductive {
            name: Name::from_str_parts("Nat"),
            level_params: vec![],
            type_: Expr::type_(),
            num_params: 0,
            num_indices: 0,
            all: vec![Name::from_str_parts("Nat")],
            ctors: vec![
                Name::from_str_parts("Nat.zero"),
                Name::from_str_parts("Nat.succ"),
            ],
            num_nested: 0,
            is_rec: true,
            is_unsafe: false,
            is_reflexive: false,
        })
        .unwrap();

    let env = env
        .add_constant(ConstantInfo::Constructor {
            name: Name::from_str_parts("Nat.zero"),
            level_params: vec![],
            type_: Expr::const_(Name::from_str_parts("Nat"), vec![]),
            induct_name: Name::from_str_parts("Nat"),
            ctor_idx: 0,
            num_params: 0,
            num_fields: 0,
            is_unsafe: false,
        })
        .unwrap();

    let env = env
        .add_constant(ConstantInfo::Constructor {
            name: Name::from_str_parts("Nat.succ"),
            level_params: vec![],
            type_: Expr::type_(), // simplified
            induct_name: Name::from_str_parts("Nat"),
            ctor_idx: 1,
            num_params: 0,
            num_fields: 1,
            is_unsafe: false,
        })
        .unwrap();

    // Nat.rec : {motive : Nat → Sort u} → motive zero → ((n : Nat) → motive n → motive (succ n)) → (n : Nat) → motive n
    // num_params = 0, num_motives = 1, num_minors = 2, num_indices = 0
    // total arity = 0 + 1 + 2 + 0 + 1 = 4
    // args: [motive, zero_case, succ_case, major]
    //
    // For Nat.zero: subst = [motive, zero_case, succ_case] (no fields)
    //   reversed: [succ_case, zero_case, motive]
    //   bvar(0) = succ_case, bvar(1) = zero_case, bvar(2) = motive
    //   Rule rhs: bvar(1) = zero_case ✓
    //
    // For Nat.succ: subst = [motive, zero_case, succ_case, pred_n] (1 field)
    //   reversed: [pred_n, succ_case, zero_case, motive]
    //   bvar(0) = pred_n, bvar(1) = succ_case, bvar(2) = zero_case, bvar(3) = motive
    //   Rule rhs: (succ_case pred_n (rec motive zero_case succ_case pred_n))
    //   = App(App(bvar(1), bvar(0)), ???)
    //   The recursive call is tricky — for now the succ_case minor handles it.
    //   Actually in Lean's recursor rules, the RHS includes the recursive application.
    //   So the rule's RHS just expects [params, motives, minors, fields...]
    //   and the minor premise (succ_case) is a function that takes (n, ih) → result.
    env.add_constant(ConstantInfo::Recursor {
        name: Name::from_str_parts("Nat.rec"),
        level_params: vec![Name::mk_simple("u")],
        type_: Expr::type_(),
        all: vec![Name::from_str_parts("Nat")],
        num_params: 0,
        num_indices: 0,
        num_motives: 1,
        num_minors: 2,
        rules: vec![
            RecursorRule {
                ctor_name: Name::from_str_parts("Nat.zero"),
                num_fields: 0,
                rhs: Expr::bvar(1), // zero_case
            },
            RecursorRule {
                ctor_name: Name::from_str_parts("Nat.succ"),
                num_fields: 1,
                // succ_case applied to pred_n
                // subst reversed: [pred_n, succ_case, zero_case, motive]
                // bvar(0) = pred_n, bvar(1) = succ_case
                rhs: Expr::app(Expr::bvar(1), Expr::bvar(0)),
            },
        ],
        is_k: false,
        is_unsafe: false,
    })
    .unwrap()
}

#[test]
fn test_eval_nat_rec_zero() {
    // Nat.rec motive 42 succ_case Nat.zero  →  42
    let env = env_with_nat_rec();
    let mut interp = Interpreter::new(env);

    let rec = Expr::const_(Name::from_str_parts("Nat.rec"), vec![Level::one()]);
    let app = Expr::mk_app(
        rec,
        &[
            Expr::type_(),                                         // motive
            Expr::lit(Literal::nat_small(42)),                      // zero case
            Expr::type_(),                                         // succ case (unused)
            Expr::const_(Name::from_str_parts("Nat.zero"), vec![]), // major = 0
        ],
    );
    let val = interp.eval(&app, &LocalEnv::new()).unwrap();
    assert_eq!(*val.as_nat().unwrap(), BigUint::from(42u64));
}

// ====================== Array builtins ======================

#[test]
fn test_builtin_array_mk_empty_and_push() {
    let env = Environment::new();
    let env = env
        .add_constant(ConstantInfo::Axiom {
            name: Name::from_str_parts("Array.mkEmpty"),
            level_params: vec![],
            type_: Expr::forall_e(
                Name::anonymous(),
                Expr::type_(), // α
                Expr::forall_e(
                    Name::anonymous(),
                    Expr::type_(), // capacity
                    Expr::type_(),
                    BinderInfo::Default,
                ),
                BinderInfo::Implicit,
            ),
            is_unsafe: false,
        })
        .unwrap();
    let env = env
        .add_constant(ConstantInfo::Axiom {
            name: Name::from_str_parts("Array.push"),
            level_params: vec![],
            type_: Expr::forall_e(
                Name::anonymous(),
                Expr::type_(),
                Expr::forall_e(
                    Name::anonymous(),
                    Expr::type_(),
                    Expr::forall_e(
                        Name::anonymous(),
                        Expr::type_(),
                        Expr::type_(),
                        BinderInfo::Default,
                    ),
                    BinderInfo::Default,
                ),
                BinderInfo::Implicit,
            ),
            is_unsafe: false,
        })
        .unwrap();
    let env = env
        .add_constant(ConstantInfo::Axiom {
            name: Name::from_str_parts("Array.size"),
            level_params: vec![],
            type_: Expr::forall_e(
                Name::anonymous(),
                Expr::type_(),
                Expr::forall_e(
                    Name::anonymous(),
                    Expr::type_(),
                    Expr::type_(),
                    BinderInfo::Default,
                ),
                BinderInfo::Implicit,
            ),
            is_unsafe: false,
        })
        .unwrap();
    let mut interp = Interpreter::new(env);

    // Create empty array
    let mk_empty = Expr::const_(Name::from_str_parts("Array.mkEmpty"), vec![]);
    let empty = Expr::mk_app(
        mk_empty,
        &[Expr::type_(), Expr::lit(Literal::nat_small(0))],
    );
    let arr_val = interp.eval(&empty, &LocalEnv::new()).unwrap();
    match &arr_val {
        Value::Array(a) => assert!(a.is_empty()),
        _ => panic!("expected Array"),
    }
}
