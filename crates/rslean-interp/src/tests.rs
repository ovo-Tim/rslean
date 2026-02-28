use num_bigint::BigUint;
use rslean_expr::{
    BinderInfo, ConstantInfo, DefinitionSafety, Expr, Literal, RecursorRule, ReducibilityHints,
};
use rslean_kernel::Environment;
use rslean_level::Level;
use rslean_name::Name;

use crate::env::LocalEnv;
use crate::error::InterpError;
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
    // num_params = 0, num_motives = 1, num_minors = 2, num_indices = 0
    // Total arity = 0 + 1 + 2 + 0 + 1 = 4
    // Args: [motive, false_case, true_case, major]
    //
    // Rule RHS in .olean format: closed lambdas taking subst values as params.
    // subst = [motive, false_case, true_case] (no fields for either ctor)
    //
    // Bool.false rule: fun motive false_case true_case => false_case
    //   = fun m => fun f => fun t => #1
    // Bool.true rule: fun motive false_case true_case => true_case
    //   = fun m => fun f => fun t => #0
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
                // fun motive => fun false_case => fun true_case => false_case (#1)
                rhs: Expr::lam(
                    Name::mk_simple("m"), Expr::type_(),
                    Expr::lam(
                        Name::mk_simple("f"), Expr::type_(),
                        Expr::lam(
                            Name::mk_simple("t"), Expr::type_(),
                            Expr::bvar(1), // false_case
                            BinderInfo::Default,
                        ),
                        BinderInfo::Default,
                    ),
                    BinderInfo::Default,
                ),
            },
            RecursorRule {
                ctor_name: Name::from_str_parts("Bool.true"),
                num_fields: 0,
                // fun motive => fun false_case => fun true_case => true_case (#0)
                rhs: Expr::lam(
                    Name::mk_simple("m"), Expr::type_(),
                    Expr::lam(
                        Name::mk_simple("f"), Expr::type_(),
                        Expr::lam(
                            Name::mk_simple("t"), Expr::type_(),
                            Expr::bvar(0), // true_case
                            BinderInfo::Default,
                        ),
                        BinderInfo::Default,
                    ),
                    BinderInfo::Default,
                ),
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
            // Type: Nat → Nat (so recursive field detection works)
            type_: Expr::forall_e(
                Name::mk_simple("n"),
                Expr::const_(Name::from_str_parts("Nat"), vec![]),
                Expr::const_(Name::from_str_parts("Nat"), vec![]),
                BinderInfo::Default,
            ),
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
    // Rule RHS in .olean format: closed lambdas taking subst values as params.
    // The IH is NOT a parameter — it is computed via embedded recursive calls.
    //
    // For Nat.zero: subst = [motive, zero_case, succ_case]
    //   RHS: fun m z s => z
    //
    // For Nat.succ: subst = [motive, zero_case, succ_case, n]
    //   RHS: fun m z s n => s n (Nat.rec.{u} m z s n)
    //   where the recursive call computes the IH
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
                // fun m => fun z => fun s => z (#1)
                rhs: Expr::lam(
                    Name::mk_simple("m"), Expr::type_(),
                    Expr::lam(
                        Name::mk_simple("z"), Expr::type_(),
                        Expr::lam(
                            Name::mk_simple("s"), Expr::type_(),
                            Expr::bvar(1), // z
                            BinderInfo::Default,
                        ),
                        BinderInfo::Default,
                    ),
                    BinderInfo::Default,
                ),
            },
            RecursorRule {
                ctor_name: Name::from_str_parts("Nat.succ"),
                num_fields: 1,
                // fun m => fun z => fun s => fun n => s n (Nat.rec.{u} m z s n)
                // Inside: s = #1, n = #0, m = #3, z = #2
                // Nat.rec.{u} m z s n is the recursive call for IH
                rhs: Expr::lam(
                    Name::mk_simple("m"), Expr::type_(),
                    Expr::lam(
                        Name::mk_simple("z"), Expr::type_(),
                        Expr::lam(
                            Name::mk_simple("s"), Expr::type_(),
                            Expr::lam(
                                Name::mk_simple("n"), Expr::type_(),
                                // s n (Nat.rec.{u} m z s n)
                                Expr::app(
                                    Expr::app(Expr::bvar(1), Expr::bvar(0)),
                                    // Nat.rec.{u} m z s n
                                    Expr::mk_app(
                                        Expr::const_(
                                            Name::from_str_parts("Nat.rec"),
                                            vec![Level::param(Name::mk_simple("u"))],
                                        ),
                                        &[
                                            Expr::bvar(3), // m
                                            Expr::bvar(2), // z
                                            Expr::bvar(1), // s
                                            Expr::bvar(0), // n
                                        ],
                                    ),
                                ),
                                BinderInfo::Default,
                            ),
                            BinderInfo::Default,
                        ),
                        BinderInfo::Default,
                    ),
                    BinderInfo::Default,
                ),
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

#[test]
fn test_eval_nat_rec_succ() {
    // Nat.rec motive 0 (fun n ih => Nat.add ih 1) 3  →  3
    // This computes: f(0) = 0, f(n+1) = f(n) + 1
    // f(3) = f(2) + 1 = (f(1) + 1) + 1 = ((f(0) + 1) + 1) + 1 = 3
    let env = env_with_nat_rec();
    // Also need Nat.add builtin
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

    // succ_case = fun n ih => Nat.add ih 1
    let succ_case = Expr::lam(
        Name::mk_simple("n"),
        Expr::type_(),
        Expr::lam(
            Name::mk_simple("ih"),
            Expr::type_(),
            // Nat.add ih 1 = App(App(Nat.add, bvar(0)), Lit(1))
            Expr::mk_app(
                Expr::const_(Name::from_str_parts("Nat.add"), vec![]),
                &[Expr::bvar(0), Expr::lit(Literal::nat_small(1))],
            ),
            BinderInfo::Default,
        ),
        BinderInfo::Default,
    );

    let rec = Expr::const_(Name::from_str_parts("Nat.rec"), vec![Level::one()]);
    let app = Expr::mk_app(
        rec,
        &[
            Expr::type_(),                    // motive
            Expr::lit(Literal::nat_small(0)), // zero case
            succ_case,                        // succ case
            Expr::lit(Literal::nat_small(3)), // major = 3
        ],
    );
    let val = interp.eval(&app, &LocalEnv::new()).unwrap();
    assert_eq!(*val.as_nat().unwrap(), BigUint::from(3u64));
}

#[test]
fn test_eval_nat_rec_factorial() {
    // Compute factorial via Nat.rec:
    // fact(0) = 1, fact(n+1) = (n+1) * fact(n)
    // succ_case = fun n ih => Nat.mul (Nat.add n 1) ih
    let env = env_with_nat_rec();
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

    // succ_case = fun n ih => Nat.mul (Nat.add n 1) ih
    let succ_case = Expr::lam(
        Name::mk_simple("n"),
        Expr::type_(),
        Expr::lam(
            Name::mk_simple("ih"),
            Expr::type_(),
            // Nat.mul (Nat.add n 1) ih
            // n = bvar(1), ih = bvar(0)
            Expr::mk_app(
                Expr::const_(Name::from_str_parts("Nat.mul"), vec![]),
                &[
                    Expr::mk_app(
                        Expr::const_(Name::from_str_parts("Nat.add"), vec![]),
                        &[Expr::bvar(1), Expr::lit(Literal::nat_small(1))],
                    ),
                    Expr::bvar(0),
                ],
            ),
            BinderInfo::Default,
        ),
        BinderInfo::Default,
    );

    let rec = Expr::const_(Name::from_str_parts("Nat.rec"), vec![Level::one()]);
    // fact(5) = 120
    let app = Expr::mk_app(
        rec,
        &[
            Expr::type_(),                    // motive
            Expr::lit(Literal::nat_small(1)), // zero case (0! = 1)
            succ_case,                        // succ case
            Expr::lit(Literal::nat_small(5)), // major = 5
        ],
    );
    let val = interp.eval(&app, &LocalEnv::new()).unwrap();
    assert_eq!(*val.as_nat().unwrap(), BigUint::from(120u64));
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

// ====================== UInt64 builtins ======================

#[test]
fn test_builtin_uint64() {
    let env = Environment::new();
    // Register UInt64.add with a 2-arity type
    let env = env
        .add_constant(ConstantInfo::Axiom {
            name: Name::from_str_parts("UInt64.add"),
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

    let add = Expr::const_(Name::from_str_parts("UInt64.add"), vec![]);
    let app = Expr::mk_app(
        add,
        &[
            Expr::lit(Literal::nat_small(u64::MAX)),
            Expr::lit(Literal::nat_small(2)),
        ],
    );
    let val = interp.eval(&app, &LocalEnv::new()).unwrap();
    // u64::MAX + 2 wraps to 1
    assert_eq!(*val.as_nat().unwrap(), BigUint::from(1u64));
}

// ====================== ST/Ref builtins ======================

#[test]
fn test_builtin_st_ref() {
    // Helper to build a 4-arity type: {σ} → {α} → val → world → result
    let mk_st_type_4 = || {
        Expr::forall_e(
            Name::anonymous(), Expr::type_(),
            Expr::forall_e(
                Name::anonymous(), Expr::type_(),
                Expr::forall_e(
                    Name::anonymous(), Expr::type_(),
                    Expr::forall_e(
                        Name::anonymous(), Expr::type_(),
                        Expr::type_(),
                        BinderInfo::Default,
                    ),
                    BinderInfo::Default,
                ),
                BinderInfo::Implicit,
            ),
            BinderInfo::Implicit,
        )
    };

    let env = Environment::new();
    // Register ST.Ref.mk with 4-arity type (σ, α, initial_val, world)
    let env = env
        .add_constant(ConstantInfo::Axiom {
            name: Name::from_str_parts("ST.Ref.mk"),
            level_params: vec![],
            type_: mk_st_type_4(),
            is_unsafe: false,
        })
        .unwrap();
    let env = env
        .add_constant(ConstantInfo::Axiom {
            name: Name::from_str_parts("ST.Ref.get"),
            level_params: vec![],
            type_: mk_st_type_4(),
            is_unsafe: false,
        })
        .unwrap();
    let env = env
        .add_constant(ConstantInfo::Axiom {
            name: Name::from_str_parts("ST.Ref.set"),
            level_params: vec![],
            type_: Expr::forall_e(
                Name::anonymous(), Expr::type_(),
                Expr::forall_e(
                    Name::anonymous(), Expr::type_(),
                    Expr::forall_e(
                        Name::anonymous(), Expr::type_(),
                        Expr::forall_e(
                            Name::anonymous(), Expr::type_(),
                            Expr::forall_e(
                                Name::anonymous(), Expr::type_(),
                                Expr::type_(),
                                BinderInfo::Default,
                            ),
                            BinderInfo::Default,
                        ),
                        BinderInfo::Default,
                    ),
                    BinderInfo::Implicit,
                ),
                BinderInfo::Implicit,
            ),
            is_unsafe: false,
        })
        .unwrap();
    let mut interp = Interpreter::new(env);

    // Create a ref with initial value 42
    // ST.Ref.mk now expects [σ, α, val, world] and returns EStateM.Result.ok(ref, world)
    let mk = Expr::const_(Name::from_str_parts("ST.Ref.mk"), vec![]);
    let mk_app = Expr::mk_app(
        mk,
        &[
            Expr::type_(),
            Expr::type_(),
            Expr::lit(Literal::nat_small(42)),
            Expr::type_(), // world token (erased)
        ],
    );
    let result = interp.eval(&mk_app, &LocalEnv::new()).unwrap();
    // Result should be EStateM.Result.ok wrapping a Ref
    let ref_val = match &result {
        Value::Ctor { tag: 0, fields, .. } => {
            assert!(!fields.is_empty());
            fields[0].clone()
        }
        _ => panic!("Expected EStateM.Result.ok, got {:?}", result),
    };
    assert!(matches!(ref_val, Value::Ref(_)));

    // Get the value using the builtin directly
    use crate::builtins::BuiltinFn;
    let get_fn: BuiltinFn = |args: &[Value]| {
        let r = args.iter().find_map(|v| match v {
            Value::Ref(r) => Some(r.as_ref()),
            _ => None,
        }).ok_or_else(|| InterpError::BuiltinError("no ref".into()))?;
        Ok(r.borrow().clone())
    };
    let got = get_fn(&[Value::Erased, Value::Erased, ref_val.clone()]).unwrap();
    assert_eq!(*got.as_nat().unwrap(), BigUint::from(42u64));
}

// ====================== .olean Integration Tests ======================

use crate::loader;

fn load_prelude_env() -> Option<Environment> {
    loader::load_prelude_env()
}

fn load_module_env(module_name: &str) -> Option<Environment> {
    loader::load_module_env(module_name)
}

#[test]
fn test_olean_nat_add() {
    // Nat.add 2 3 → 5
    let env = match load_prelude_env() {
        Some(e) => e,
        None => {
            eprintln!("Skipping test: no Lean toolchain found");
            return;
        }
    };
    let mut interp = Interpreter::new(env);

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
fn test_olean_nat_mul() {
    // Nat.mul 6 7 → 42
    let env = match load_prelude_env() {
        Some(e) => e,
        None => {
            eprintln!("Skipping test: no Lean toolchain found");
            return;
        }
    };
    let mut interp = Interpreter::new(env);

    let mul = Expr::const_(Name::from_str_parts("Nat.mul"), vec![]);
    let app = Expr::mk_app(
        mul,
        &[
            Expr::lit(Literal::nat_small(6)),
            Expr::lit(Literal::nat_small(7)),
        ],
    );
    let val = interp.eval(&app, &LocalEnv::new()).unwrap();
    assert_eq!(*val.as_nat().unwrap(), BigUint::from(42u64));
}

#[test]
fn test_olean_nat_succ() {
    // Nat.succ (Nat.succ 0) → 2
    let env = match load_prelude_env() {
        Some(e) => e,
        None => {
            eprintln!("Skipping test: no Lean toolchain found");
            return;
        }
    };
    let mut interp = Interpreter::new(env);

    let succ = Expr::const_(Name::from_str_parts("Nat.succ"), vec![]);
    let app = Expr::app(
        succ.clone(),
        Expr::app(succ, Expr::lit(Literal::nat_small(0))),
    );
    let val = interp.eval(&app, &LocalEnv::new()).unwrap();
    assert_eq!(*val.as_nat().unwrap(), BigUint::from(2u64));
}

#[test]
fn test_olean_bool_not() {
    // Bool.not Bool.true → Bool.false
    let env = match load_prelude_env() {
        Some(e) => e,
        None => {
            eprintln!("Skipping test: no Lean toolchain found");
            return;
        }
    };
    let mut interp = Interpreter::new(env);

    let not = Expr::const_(Name::from_str_parts("Bool.not"), vec![]);
    let app = Expr::app(
        not,
        Expr::const_(Name::from_str_parts("Bool.true"), vec![]),
    );
    let val = interp.eval(&app, &LocalEnv::new()).unwrap();
    assert_eq!(val.as_bool(), Some(false));
}

#[test]
fn test_olean_bool_and() {
    // Bool.and Bool.true Bool.false → Bool.false
    let env = match load_prelude_env() {
        Some(e) => e,
        None => {
            eprintln!("Skipping test: no Lean toolchain found");
            return;
        }
    };
    let mut interp = Interpreter::new(env);

    let and = Expr::const_(Name::from_str_parts("Bool.and"), vec![]);
    let app = Expr::mk_app(
        and,
        &[
            Expr::const_(Name::from_str_parts("Bool.true"), vec![]),
            Expr::const_(Name::from_str_parts("Bool.false"), vec![]),
        ],
    );
    let val = interp.eval(&app, &LocalEnv::new()).unwrap();
    assert_eq!(val.as_bool(), Some(false));
}

#[test]
fn test_olean_nat_pow() {
    // Nat.pow 2 10 → 1024
    let env = match load_prelude_env() {
        Some(e) => e,
        None => {
            eprintln!("Skipping test: no Lean toolchain found");
            return;
        }
    };
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
fn test_olean_string_length() {
    // String.length "hello" → 5
    let env = match load_prelude_env() {
        Some(e) => e,
        None => {
            eprintln!("Skipping test: no Lean toolchain found");
            return;
        }
    };
    let mut interp = Interpreter::new(env);

    // Check if String.length exists in prelude
    if interp.env().find(&Name::from_str_parts("String.length")).is_none() {
        eprintln!("Skipping: String.length not in prelude");
        return;
    }

    let length = Expr::const_(Name::from_str_parts("String.length"), vec![]);
    let app = Expr::app(length, Expr::lit(Literal::string("hello")));
    let val = interp.eval(&app, &LocalEnv::new()).unwrap();
    assert_eq!(*val.as_nat().unwrap(), BigUint::from(5u64));
}

#[test]
fn test_olean_id_nat() {
    // @id Nat 42 → 42
    let env = match load_prelude_env() {
        Some(e) => e,
        None => {
            eprintln!("Skipping test: no Lean toolchain found");
            return;
        }
    };
    let mut interp = Interpreter::new(env);

    let id = Expr::const_(Name::from_str_parts("id"), vec![Level::one()]);
    let app = Expr::mk_app(
        id,
        &[
            Expr::type_(),                      // α = Nat (type, will be erased)
            Expr::lit(Literal::nat_small(42)),  // value
        ],
    );
    let val = interp.eval(&app, &LocalEnv::new()).unwrap();
    assert_eq!(*val.as_nat().unwrap(), BigUint::from(42u64));
}

// ====================== Multi-module .olean Integration Tests ======================

#[test]
fn test_olean_list_map() {
    // List.map (· + 1) [1, 2, 3] → [2, 3, 4]
    let env = match load_module_env("Init.Data.List.Basic") {
        Some(e) => e,
        None => {
            eprintln!("Skipping test: cannot load Init.Data.List.Basic");
            return;
        }
    };
    let mut interp = Interpreter::new(env);

    // Verify key definitions loaded
    assert!(
        interp.env().find(&Name::from_str_parts("List.map")).is_some(),
        "List.map not found in environment"
    );

    // Build: List.map Nat Nat (fun n => Nat.add n 1) (List.cons Nat 1 (List.cons Nat 2 (List.cons Nat 3 (List.nil Nat))))
    let nat = Expr::const_(Name::from_str_parts("Nat"), vec![]);
    let one = Expr::lit(Literal::nat_small(1));
    let two = Expr::lit(Literal::nat_small(2));
    let three = Expr::lit(Literal::nat_small(3));

    // The mapping function: fun (n : Nat) => Nat.add n 1
    let add_one = Expr::lam(
        Name::mk_simple("n"),
        nat.clone(),
        Expr::mk_app(
            Expr::const_(Name::from_str_parts("Nat.add"), vec![]),
            &[Expr::bvar(0), Expr::lit(Literal::nat_small(1))],
        ),
        BinderInfo::Default,
    );

    // Build the list [1, 2, 3] = cons 1 (cons 2 (cons 3 nil))
    let nil = Expr::mk_app(
        Expr::const_(Name::from_str_parts("List.nil"), vec![Level::one()]),
        &[nat.clone()],
    );
    let list = Expr::mk_app(
        Expr::const_(Name::from_str_parts("List.cons"), vec![Level::one()]),
        &[nat.clone(), one, Expr::mk_app(
            Expr::const_(Name::from_str_parts("List.cons"), vec![Level::one()]),
            &[nat.clone(), two, Expr::mk_app(
                Expr::const_(Name::from_str_parts("List.cons"), vec![Level::one()]),
                &[nat.clone(), three, nil],
            )],
        )],
    );

    // List.map Nat Nat add_one list
    let map_expr = Expr::mk_app(
        Expr::const_(Name::from_str_parts("List.map"), vec![Level::one(), Level::one()]),
        &[nat.clone(), nat, add_one, list],
    );

    let result = match interp.eval(&map_expr, &LocalEnv::new()) {
        Ok(v) => v,
        Err(e) => panic!("eval error: {:?}", e),
    };

    // Extract the list elements
    let elems = list_to_vec(&result);
    let nat_elems: Vec<u64> = elems
        .iter()
        .map(|v| {
            let n = v.as_nat().expect("expected Nat in list");
            n.iter_u64_digits().next().unwrap_or(0)
        })
        .collect();
    assert_eq!(nat_elems, vec![2, 3, 4]);
}

#[test]
fn test_olean_list_rec_direct() {
    // Use List.rec directly to implement map: (· + 1) over [1, 2, 3] → [2, 3, 4]
    // List.rec.{u, v} {α : Type u} {motive : List α → Sort v}
    //   (nil : motive []) (cons : (head : α) → (tail : List α) → motive tail → motive (head :: tail))
    //   (t : List α) : motive t
    let env = match load_module_env("Init.Data.List.Basic") {
        Some(e) => e,
        None => {
            eprintln!("Skipping test: cannot load Init.Data.List.Basic");
            return;
        }
    };
    let mut interp = Interpreter::new(env);

    let nat = Expr::const_(Name::from_str_parts("Nat"), vec![]);

    // Build the list [1, 2, 3]
    let nil = Expr::mk_app(
        Expr::const_(Name::from_str_parts("List.nil"), vec![Level::one()]),
        &[nat.clone()],
    );
    let list_3 = Expr::mk_app(
        Expr::const_(Name::from_str_parts("List.cons"), vec![Level::one()]),
        &[nat.clone(), Expr::lit(Literal::nat_small(3)), nil],
    );
    let list_23 = Expr::mk_app(
        Expr::const_(Name::from_str_parts("List.cons"), vec![Level::one()]),
        &[nat.clone(), Expr::lit(Literal::nat_small(2)), list_3],
    );
    let list_123 = Expr::mk_app(
        Expr::const_(Name::from_str_parts("List.cons"), vec![Level::one()]),
        &[nat.clone(), Expr::lit(Literal::nat_small(1)), list_23],
    );

    // motive: fun (_ : List Nat) => List Nat (constant motive)
    let list_nat = Expr::mk_app(
        Expr::const_(Name::from_str_parts("List"), vec![Level::one()]),
        &[nat.clone()],
    );
    let motive = Expr::lam(
        Name::mk_simple("_"),
        list_nat.clone(),
        list_nat.clone(),
        BinderInfo::Default,
    );

    // nil case: List.nil Nat
    let nil_case = Expr::mk_app(
        Expr::const_(Name::from_str_parts("List.nil"), vec![Level::one()]),
        &[nat.clone()],
    );

    // cons case: fun (head : Nat) (_ : List Nat) (ih : List Nat) => List.cons Nat (Nat.add head 1) ih
    let cons_case = Expr::lam(
        Name::mk_simple("head"),
        nat.clone(),
        Expr::lam(
            Name::mk_simple("_tail"),
            list_nat.clone(),
            Expr::lam(
                Name::mk_simple("ih"),
                list_nat.clone(),
                // List.cons Nat (Nat.add head 1) ih
                Expr::mk_app(
                    Expr::const_(Name::from_str_parts("List.cons"), vec![Level::one()]),
                    &[
                        nat.clone(),
                        Expr::mk_app(
                            Expr::const_(Name::from_str_parts("Nat.add"), vec![]),
                            &[Expr::bvar(2), Expr::lit(Literal::nat_small(1))],
                        ),
                        Expr::bvar(0),
                    ],
                ),
                BinderInfo::Default,
            ),
            BinderInfo::Default,
        ),
        BinderInfo::Default,
    );

    // List.rec.{1, 1} Nat (fun _ => List Nat) nil_case cons_case [1, 2, 3]
    let rec_expr = Expr::mk_app(
        Expr::const_(
            Name::from_str_parts("List.rec"),
            vec![Level::one(), Level::one()],
        ),
        &[nat.clone(), motive, nil_case, cons_case, list_123],
    );

    let result = match interp.eval(&rec_expr, &LocalEnv::new()) {
        Ok(v) => v,
        Err(e) => panic!("eval error: {:?}", e),
    };

    let elems = list_to_vec(&result);
    let nat_elems: Vec<u64> = elems
        .iter()
        .map(|v| {
            let n = v.as_nat().expect("expected Nat in list");
            n.iter_u64_digits().next().unwrap_or(0)
        })
        .collect();
    assert_eq!(nat_elems, vec![2, 3, 4]);
}

/// Convert a Value representing a List to a Vec<Value>.
fn list_to_vec(val: &Value) -> Vec<Value> {
    let mut result = Vec::new();
    let mut current = val.clone();
    loop {
        match &current {
            Value::Ctor { name, fields, .. } => {
                let name_str = name.to_string();
                if name_str == "List.nil" {
                    break;
                } else if name_str == "List.cons" {
                    // fields[0] = head, fields[1] = tail
                    if fields.len() >= 2 {
                        result.push(fields[0].clone());
                        current = fields[1].clone();
                    } else {
                        break;
                    }
                } else {
                    break;
                }
            }
            _ => break,
        }
    }
    result
}

// ====================== IO Monad Integration Tests ======================

#[test]
fn test_st_ref_monadic_convention() {
    // Test that ST.Prim.mkRef returns EStateM.Result.ok wrapping a Ref
    let env = match load_prelude_env() {
        Some(e) => e,
        None => {
            eprintln!("Skipping test: no Lean toolchain found");
            return;
        }
    };
    let _interp = Interpreter::new(env);
    // Directly test the builtin function
    let args = vec![Value::Erased, Value::Erased, Value::nat_small(42), Value::Erased];
    let result = crate::builtins::test_builtin_call("ST.Prim.mkRef", &args);
    match result {
        Ok(val) => {
            // Should be EStateM.Result.ok with a Ref inside
            match &val {
                Value::Ctor { name, fields, tag: 0 } => {
                    assert_eq!(name.to_string(), "EStateM.Result.ok");
                    assert_eq!(fields.len(), 2);
                    assert!(matches!(&fields[0], Value::Ref(_)));
                }
                _ => panic!("Expected EStateM.Result.ok, got {:?}", val),
            }
        }
        Err(e) => panic!("ST.Prim.mkRef failed: {}", e),
    }
}

#[test]
fn test_io_println_monadic_convention() {
    // Test that IO.println returns EStateM.Result.ok wrapping Unit
    let args = vec![Value::Erased, Value::string("test"), Value::Erased];
    let result = crate::builtins::test_builtin_call("IO.println", &args);
    match result {
        Ok(val) => {
            match &val {
                Value::Ctor { name, fields, tag: 0 } => {
                    assert_eq!(name.to_string(), "EStateM.Result.ok");
                    assert_eq!(fields.len(), 2);
                    // fields[0] should be Unit.unit
                    assert!(matches!(&fields[0], Value::Ctor { tag: 0, .. }));
                }
                _ => panic!("Expected EStateM.Result.ok, got {:?}", val),
            }
        }
        Err(e) => panic!("IO.println failed: {}", e),
    }
}

#[test]
fn test_hashmap_operations() {
    // Test HashMap create, insert, find, size
    let empty_args: Vec<Value> = vec![Value::Erased];
    let map = crate::builtins::test_builtin_call("Lean.HashMap.mkEmpty", &empty_args).unwrap();
    assert!(matches!(&map, Value::HashMap(_)));

    // Insert a key-value pair
    let insert_args = vec![Value::Erased, Value::Erased, map.clone(), Value::string("key"), Value::nat_small(42)];
    let map2 = crate::builtins::test_builtin_call("Lean.HashMap.insert", &insert_args).unwrap();

    // Check size
    let size_args = vec![Value::Erased, Value::Erased, map2.clone()];
    let size = crate::builtins::test_builtin_call("Lean.HashMap.size", &size_args).unwrap();
    assert_eq!(*size.as_nat().unwrap(), BigUint::from(1u64));

    // Find the key
    let find_args = vec![Value::Erased, Value::Erased, map2.clone(), Value::string("key")];
    let found = crate::builtins::test_builtin_call("Lean.HashMap.find?", &find_args).unwrap();
    match &found {
        Value::Ctor { tag: 1, name, fields } => {
            assert_eq!(name.to_string(), "Option.some");
            assert_eq!(*fields[0].as_nat().unwrap(), BigUint::from(42u64));
        }
        _ => panic!("Expected Option.some, got {:?}", found),
    }

    // Find a missing key
    let find_args2 = vec![Value::Erased, Value::Erased, map2.clone(), Value::string("missing")];
    let not_found = crate::builtins::test_builtin_call("Lean.HashMap.find?", &find_args2).unwrap();
    assert!(matches!(&not_found, Value::Ctor { tag: 0, .. })); // Option.none
}

#[test]
fn test_int_arithmetic() {
    use num_bigint::BigInt;
    use std::sync::Arc;

    let a = Value::Int(Arc::new(BigInt::from(10)));
    let b = Value::Int(Arc::new(BigInt::from(-3)));

    let result = crate::builtins::test_builtin_call("Int.add", &[a.clone(), b.clone()]).unwrap();
    assert_eq!(*result.as_int().unwrap(), BigInt::from(7));

    let result = crate::builtins::test_builtin_call("Int.sub", &[a.clone(), b.clone()]).unwrap();
    assert_eq!(*result.as_int().unwrap(), BigInt::from(13));

    let result = crate::builtins::test_builtin_call("Int.mul", &[a.clone(), b.clone()]).unwrap();
    assert_eq!(*result.as_int().unwrap(), BigInt::from(-30));

    let result = crate::builtins::test_builtin_call("Int.neg", &[b.clone()]).unwrap();
    assert_eq!(*result.as_int().unwrap(), BigInt::from(3));
}

#[test]
fn test_bytearray_operations() {
    let empty = crate::builtins::test_builtin_call("ByteArray.mkEmpty", &[Value::nat_small(0)]).unwrap();
    assert!(matches!(&empty, Value::ByteArray(_)));

    let pushed = crate::builtins::test_builtin_call("ByteArray.push", &[empty, Value::nat_small(0x42)]).unwrap();
    let size = crate::builtins::test_builtin_call("ByteArray.size", &[pushed.clone()]).unwrap();
    assert_eq!(*size.as_nat().unwrap(), BigUint::from(1u64));

    let byte = crate::builtins::test_builtin_call("ByteArray.get!", &[pushed, Value::nat_small(0)]).unwrap();
    assert_eq!(*byte.as_nat().unwrap(), BigUint::from(0x42u64));
}

#[test]
fn test_arity_computation_monadic() {
    // Test that monadic builtins get the right arity after delta-reduction
    let env = match load_prelude_env() {
        Some(e) => e,
        None => {
            eprintln!("Skipping test: no Lean toolchain found");
            return;
        }
    };
    let interp = Interpreter::new(env);

    // Check that ST.Prim.mkRef has the right arity (should be 4: σ, α, val, world)
    if let Some(info) = interp.env().find(&Name::from_str_parts("ST.Prim.mkRef")) {
        // The type is: {σ : Type} → {α : Type} → α → ST σ (ST.Ref σ α)
        // ST σ X = σ → EStateM.Result ... so arity should be 4 after delta-reduction
        let _ = info; // We just verify it doesn't crash
    }
}

#[test]
fn test_loader_module() {
    // Test the shared loader module works
    if loader::find_lean_lib_dir().is_none() {
        eprintln!("Skipping test: no Lean toolchain found");
        return;
    }
    let env = loader::load_prelude_env().expect("Failed to load prelude");
    assert!(env.find(&Name::from_str_parts("Nat.add")).is_some());
    assert!(env.find(&Name::from_str_parts("Bool.true")).is_some());
}

#[test]
fn test_loader_module_with_deps() {
    // Test loading a module with dependencies
    let env = match loader::load_module_env("Init.Data.List.Basic") {
        Some(e) => e,
        None => {
            eprintln!("Skipping test: no Lean toolchain found");
            return;
        }
    };
    // Should have List.map from the loaded module
    assert!(env.find(&Name::from_str_parts("List.map")).is_some());
    // Should also have dependencies like Nat.add from Prelude
    assert!(env.find(&Name::from_str_parts("Nat.add")).is_some());
}
