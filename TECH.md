# RSLean Technical Design

Detailed design documentation for each component. See [PLAN.md](PLAN.md) for
project overview, current status, and roadmap.

---

## Core Data Types

### Name (`rslean-name`)

From Lean 4 `src/kernel/name.h`, 3 constructors:

```rust
enum Name {
    Anonymous,
    Str { prefix: Arc<Name>, s: InternedString },
    Num { prefix: Arc<Name>, n: u64 },
}
```

Design decisions:
- **Arc-based sharing** — names are immutable and shared extensively
- **MurmurHash2** 64-bit seed-11 matching Lean 4 exactly — required for
  .olean compatibility and correct hash-based lookups

### Level (`rslean-level`)

From Lean 4 `src/kernel/level.h`, 6 constructors:

```rust
enum Level {
    Zero,
    Succ(Arc<Level>),
    Max(Arc<Level>, Arc<Level>),
    IMax(Arc<Level>, Arc<Level>),
    Param(Name),
    MVar(LMVarId),
}
```

Design decisions:
- **Smart constructors** `max()`/`imax()` normalize on construction
- Full `is_equivalent`, `is_geq` comparisons

### Expr (`rslean-expr`)

From Lean 4 `src/kernel/expr.h`, 12 constructors + cached metadata:

```rust
enum ExprKind {
    BVar(u64),                                 // de Bruijn index
    FVar(FVarId),                              // free variable
    MVar(MVarId),                              // metavariable
    Sort(Level),                               // Prop / Type u
    Const(Name, Vec<Level>),                   // named constant
    App(Expr, Expr),                           // function application
    Lam(Name, Expr, Expr, BinderInfo),         // lambda
    ForallE(Name, Expr, Expr, BinderInfo),     // dependent function type
    LetE(Name, Expr, Expr, Expr, bool),        // let binding
    Lit(Literal),                              // nat/string literal
    MData(MData, Expr),                        // metadata
    Proj(Name, u64, Expr),                     // structure projection
}

struct Expr {
    kind: Arc<ExprKind>,
    data: ExprData,  // cached: hash, flags, loose_bvar_range
}
```

**BinderInfo**: `Default | Implicit | StrictImplicit | InstImplicit`

### ConstantInfo

Declarations stored in the kernel Environment:

```rust
enum ConstantInfo {
    Defn   { name, levelParams, type_, value, safety },
    Thm    { name, levelParams, type_, value },
    Axiom  { name, levelParams, type_, isUnsafe },
    Opaque { name, levelParams, type_, value, isUnsafe },
    Induct { name, levelParams, type_, numParams, numIndices, ctors, isRec, ... },
    Ctor   { name, levelParams, type_, inductName, ctorIdx, numParams, numFields },
    Rec    { name, levelParams, type_, numParams, numIndices, numMotives, ... },
    Quot   { name, levelParams, type_, kind },
}
```

**ConstantInfo layout** (from Lean 4 object model, needed for .olean deserialization):
- `AxiomVal`: 1 obj (ConstantVal) + 1 scalar u8 (isUnsafe)
- `DefinitionVal`: 4 obj (cv, value, hints, safety, all) + 0 scalar
- `InductiveVal`: 6 obj (cv, numParams, numIndices, all, ctors, numNested) + 3 scalar u8
- `ConstructorVal`: 5 obj (cv, induct, cidx, numParams, numFields) + 1 scalar u8
- `RecursorVal`: 7 obj (cv, all, numParams, numIndices, numMotives, numMinors, rules) + 2 scalar u8

---

## Kernel (`rslean-kernel`)

Port from C++ `src/kernel/type_checker.cpp` (~2.2K lines).

Core functions:
- `infer_type(expr) -> Expr` — infer the type of an expression
- `is_def_eq(a, b) -> bool` — check definitional equality
- `whnf(expr) -> Expr` — reduce to weak head normal form
- `check(decl)` — validate a declaration before adding to environment
- `add_inductive(decl)` — validate and add inductive type declarations

Key subsystems:
- WHNF reduction engine (beta, delta, iota, zeta reduction)
- Definitional equality with caching (`EquivManager`)
- Inductive type validation (~1.5K lines in `inductive.cpp`)
- Quotient type support (~180 lines in `quot.cpp`)
- Instantiation and abstraction utilities

Design decisions:
- `Environment` stores `HashMap<Name, ConstantInfo>`
- `TypeChecker` uses `local_ctx: FxHashMap<Name, Expr>` for FVar lookup during inference

### Known gaps

- **Inductive type checking** has a placeholder — `check_and_add` accepts
  inductives without validation (`add_constant_unchecked`). Full
  `add_inductive` (constructor positivity, type well-formedness, recursor
  generation) is deferred.
- **MData** `ofInt` and `ofSyntax` DataValue variants are skipped during
  deserialization.

---

## .olean Reader (`rslean-olean`)

Parses Lean 4's binary module format (v2):

```rust
ModuleData {
    imports: Vec<Import>,
    constNames: Vec<Name>,
    constants: Vec<ConstantInfo>,
    extraConstNames: Vec<Name>,
    entries: Vec<ModuleEntry>,
}
```

Key implementation details:
- `CompactedRegion` deserialization (contiguous object memory)
- Recursive object graph walker
- Offset resolution: `stored - (base_addr + 88)`
- GMP + native bignum support for Lean's `Nat` literals
- Iterative App/Lam/ForallE/LetE unrolling to avoid stack overflow on
  deeply nested expressions

---

## Lexer (`rslean-lexer`)

Tokenizes Lean 4 source into `Vec<Token>` (eager, not lazy):

- Identifiers with hierarchical `.` names and Unicode support
- Numeric literals (decimal, hex `0x`, binary `0b`, octal `0o`, with `_` separators)
- String literals with interpolation markers (`s!"...{expr}..."`)
- Guillemet identifiers (`«name with spaces»`)
- Operators and punctuation (~90 TokenKind variants)
- Keywords (90 Keyword enum variants with `from_str`/`as_str`)
- Comments: line `--`, block `/- -/` (nested), doc `/-! -/`, `/-- -/`
- Scientific literals for Float

---

## Syntax Types (`rslean-syntax`)

```rust
enum Syntax {
    Missing,                                            // error recovery
    Atom { info: SourceInfo, val: AtomVal },            // tokens
    Ident { info: SourceInfo, raw: String, val: Name }, // identifiers
    Node { info: SourceInfo, kind: SyntaxNodeKind, children: Vec<Syntax> },
}

enum AtomVal {
    Keyword(String),
    NatLit(u64),
    StrLit(String),
    CharLit(char),
    ScientificLit(String),
    Punct(String),
}
```

Design decision: **Enum-based `SyntaxNodeKind`** (~100 variants) instead of
Name-based like Lean 4, for exhaustive pattern matching and compiler-checked
coverage.

Spans: `BytePos = u32`, `Span { start: BytePos, end: BytePos }`,
`SourceInfo { leading: Option<Span>, span: Span, trailing: Option<Span> }`.

---

## Parser (`rslean-parser`)

Hand-written recursive descent parser with Pratt parsing for operators.

### Architecture

```
parser.rs  — Parser struct, parse_module(), parse_command() dispatch
expr.rs    — parse_expr(), parse_atom(), parse_infix() (Pratt), binders
command.rs — parse_declaration(), parse_structure(), parse_inductive()
pratt.rs   — Binding power tables
```

All files extend the same `Parser` struct via `impl Parser` blocks.

### Expression Parser

`parse_expr()` → `parse_unary()` → `parse_infix(lhs, min_bp)`

Pratt infix operator precedences (from Lean 4 `Init/Notation.lean`):
- `+` / `-`: 65
- `*` / `/` / `%`: 70
- `^`: 80
- `::`: 67 (right-associative)
- `<` / `>` / `≤` / `≥` / `==` / `!=`: 50
- `&&` / `∧`: 35
- `||` / `∨`: 30
- `→`: 25 (right-associative)
- `<$>`: 100
- `>>=`: 55
- `>>` / `*>`: 60
- `<|>`: 20

Named precedence levels: max=1024, arg=1023, lead=1022, min=10, min1=11.

### Atom parsing

Handles: identifiers, numeric/string/char literals, Sort/Type/Prop,
true/false/sorry, holes (`_`, `?x`), parenthesized expressions, tuples,
anonymous constructors `⟨...⟩`, list literals `[...]`, struct instances
`{ field := val }`, interpolated strings, `@` explicit application.

### Binder parsing

All Lean 4 binder types:
- Explicit: `(x : T)`
- Implicit: `{x : T}`
- Instance: `[T]`
- Strict implicit: `⦃x : T⦄` / `{{x : T}}`
- Mixed binder lists with optional types

### Command dispatch

Full Lean 4 command set: def/theorem/lemma/abbrev/opaque/instance/example/axiom,
structure, class (including `class inductive`), inductive, mutual,
namespace/section/end, open/export, variable, universe, set_option (standalone
and `... in`), attribute, #check/#eval/#print/#reduce/#synth/#exit,
notation/prefix/infix/infixl/infixr/postfix/macro/syntax/elab.

Modifiers: private/protected/public/noncomputable/unsafe/partial/nonrec.

### Error recovery

- `Syntax::Missing` nodes for expected-but-absent syntax
- Progress guards on every loop (save `self.pos`, advance if stuck)
- `parse_notation`/`parse_syntax` consume tokens greedily until next
  command at column 0 (treats user-defined syntax bodies as opaque)

### Init/Prelude.lean result

5966 lines, 232KB: 1415 commands extracted, 535 errors (9% rate), <0.1s.

---

## Interpreter (`rslean-interp`)

Tree-walking interpreter that evaluates Lean kernel `Expr` values directly
(no bytecode compilation).

### Value Representation

```rust
pub enum Value {
    Nat(Arc<BigUint>),
    Int(Arc<BigInt>),
    String(Arc<str>),
    Ctor { tag: u32, name: Name, fields: Vec<Value> },
    Closure { func: FuncRef, captured: Vec<Value>, remaining_arity: u32 },
    Array(Arc<Vec<Value>>),
    ByteArray(Arc<Vec<u8>>),
    HashMap(Arc<HashMap<Value, Value>>),
    Ref(Arc<RwLock<Value>>),
    Environment(Arc<Environment>),
    Erased,
    KernelExpr(Expr),
}

pub enum FuncRef {
    Definition(Name, Vec<Level>),
    Lambda(Expr, LocalEnv),
    Builtin(Name),
    CtorFn { name: Name, tag: u32, num_params: u32, num_fields: u32 },
    RecursorFn(Name, Vec<Level>),
    RunAction,                    // BaseIO.asTask
    ChainTask,                    // BaseIO.chainTask
    ArrayFoldlMLoop,              // Array.foldlM.loop
    ArrayFoldlMCont,              // continuation for above
}
```

Design decisions:
- `Ctor` includes `name: Name` (needed for recursor rule matching by constructor name)
- `Closure` uses `FuncRef` + `captured` args (supports partial application of
  builtins and constructors, not just lambdas)
- `Erased` replaces separate type/proof handling — ForallE, Sort, MVar all
  evaluate to Erased (computationally irrelevant)
- `KernelExpr` — opaque kernel objects passed through to elaborator bridge
- No `Thunk` variant yet (lazy evaluation deferred)
- `Int`, `ByteArray`, `HashMap`, `Ref`, `Environment` added for IO runtime

No `lean_inc` / `lean_dec`. Rust's ownership handles everything:
- `Vec<Value>` for constructor fields — dropped when unreachable
- `Arc<T>` for shared data — reference counted by Rust (safe, cycle-free)
- `Clone` is cheap (Arc bumps refcount atomically)

### Core Evaluation

`eval(expr, local_env) -> InterpResult<Value>`:

| ExprKind                    | Action                                                      |
| --------------------------- | ----------------------------------------------------------- |
| `Lit(Nat(n))`               | `Value::Nat(n)`                                             |
| `Lit(Str(s))`               | `Value::String(s)`                                          |
| `BVar(i)`                   | `local_env.lookup(i)`                                       |
| `Lam(_, _, body, _)`        | `Value::Closure { func: Lambda(body, env), arity: 1 }`      |
| `LetE(_, _, val, body, _)`  | Evaluate val, push onto env, evaluate body                  |
| `App(f, a)`                 | Evaluate f and a, apply (beta-reduce or accumulate partial) |
| `Const(name, levels)`       | Check builtins first, then `eval_const(name, levels)`       |
| `ForallE / Sort / MVar`     | `Value::Erased`                                             |
| `MData(_, e)`               | Evaluate e (metadata is transparent)                        |
| `Proj(struct_name, idx, e)` | Evaluate e to Ctor, return `fields[idx]`                    |
| `FVar(_)`                   | `Value::KernelExpr(expr)` (for elaborator bridge)           |

**App chain flattening:** Left-spine App chains are flattened into iterative
evaluation to avoid O(N) native recursion depth on deeply nested do-notation.

**LetE chain flattening:** Sequential let-binding chains evaluated iteratively.

### Builtins (272 total)

Registered as `fn(&[Value]) -> InterpResult<Value>` in `FxHashMap<Name, BuiltinFn>`,
keyed by Lean declaration name (e.g., `Nat.add`).

Arity computed automatically by counting ForallE binders in the constant's type.

| Category                | Count | Examples                                                  |
| ----------------------- | ----- | --------------------------------------------------------- |
| Nat                     | 18    | add, sub, mul, div, mod, pow, gcd, beq, ble, decEq       |
| String                  | 13    | append, length, mk, push, utf8ByteSize, utf8GetAux       |
| Bool                    | 1     | decEq                                                    |
| Array                   | 5+    | mkEmpty, push, size, get!, set!, uset, foldlM.loop        |
| UInt32/64               | 20    | ofNat, toNat, add, sub, mul, div, mod, xor, shiftRight   |
| UInt8/16/USize          | 8     | ofNat, toNat, land                                       |
| Char                    | 2     | ofNat, isWhitespace                                      |
| ST.Ref                  | 4     | get, set, swap, new                                      |
| IO                      | 3+    | checkCanceled, withIsolatedStreams, catchExceptions        |
| Lean.Expr               | ~25   | Tag predicates, field accessors, constructors, equal      |
| Lean.Name               | ~10   | mk, append, hash, beq, quickCmp                          |
| Lean.Level              | ~10   | mkSucc, mkMax, mkIMax, mkParam, normalize                |
| Environment             | ~10   | find?, contains, isConstructor, addDeclCore               |
| Kernel bridge           | 4     | isDefEq, whnf, check, addDeclCore                        |
| Expr manipulation       | 7     | instantiate1, abstract, liftLooseBVars, etc.              |
| Misc                    | ~20   | ite/dite short-circuit, ptrAddrUnsafe, strictOr/And       |

Decidable results use `Ctor { tag: 0 = isFalse, tag: 1 = isTrue }`.

### Iota Reduction

When a recursor is fully applied:
1. Identify major premise from argument position
2. Evaluate major premise to `Value::Ctor { tag, name, fields }`
3. Special-case Nat: `0` → `Nat.zero`, `n+1` → `Nat.succ(n-1)`
4. Find matching `RecursorRule` by constructor name
5. Build substitution: params + motives + minors + constructor fields
6. Push substitution into `LocalEnv` (forward order, fields at bvar(0))
7. Evaluate the rule's RHS with substitution environment

Key insight: **Lambda-wrapped iota RHS.** In .olean files, recursor rule RHS
expressions are closed lambdas that take substitution (params, motives, minors,
fields) as explicit parameters. The iota reducer evaluates RHS to a closure
and applies substitution values one by one.

Key insight: **IH via embedded recursive calls.** The .olean recursor rule RHS
does NOT receive induction hypotheses as parameters. Instead, the RHS body
contains embedded recursive calls to the recursor that compute IH during
evaluation. This is how Lean 4 kernel represents recursor computation rules.

### Module Loading

BFS dependency-resolving loader:
- `find_lean_lib_dir()` — locate toolchain .olean files
- `resolve_module()` — module name → file path
- `load_env_with_deps()` — load module + all transitive dependencies
- `load_lean_library()` — load full Lean library (Init + Lean.*)
- 174,018 constants loaded in ~3.5s

### Compiler Auxiliary Detection

`is_compiler_aux` identifies generated definitions that should not be evaluated:
`_cstage1`, `_cstage2`, `_closed_N`, `_lambda_N`, `_neutral`, `_rarg`
→ all return `Value::Erased`.

---

## Elaboration Bridge

The interpreted elaborator (Mode A) lives inside `rslean-interp`.

### `process_lean_input(source, filename)`

Entry point for elaborating `.lean` source code:

1. Constructs Lean interpreter values:
   - `InputContext` (source string + filename)
   - `CommandState` (Environment + empty MessageLog + options)
   - `ParserState` (position = 0)
   - `FrontendContext`, `FrontendState`
2. Calls `Lean.Elab.Frontend.processCommands` in the interpreter
3. Uses **Lean's own parser** loaded from .olean (NOT the Rust parser)
4. Returns `EStateM.Result` with final state containing updated Environment

### Runtime Fixes (elaboration path)

Critical fixes discovered during elaboration integration:

- **O(n²) Environment building** → `Arc::make_mut` for constant insertion
- **`ite`/`dite` short-circuit** — avoids evaluating both branches
- **`io_promise_new`** — extracts Nonempty default from Ctor fields[0]
- **`task_get`** — returns value directly (pure function, not IO)
- **`BaseIO.asTask`** → `FuncRef::RunAction`: runs IO action synchronously
- **`BaseIO.chainTask`** → `FuncRef::ChainTask`: chains Task with continuation
- **`EStateM.Result` applied as function** — pass-through in `apply()`
- **Iterative `Nat.rec`** — bottom-up computation, constant eval depth
- **`eval_proj` returns Erased** for out-of-range projections
- **`String.rec`** handles `Value::String` (iota reduction)
- **`unwrap_nat` for multi-field Ctors** (UInt32/BitVec/Fin unwrapping)

### Elaboration results

```
#check Nat                          →  7,292 steps ✓
#check Nat.add                      →  5,759 steps ✓
#check @List.map                    →  6,307 steps ✓
def foo := 42                       →  5,485 steps ✓
theorem t1 : True := trivial        →  9,595 steps ✓
theorem t2 : 1 + 1 = 2 := rfl      →  9,869 steps ✓
example : 2 + 3 = 5 := by decide   → 10,691 steps ✓
induction tactic with simp          → 33,587 steps ✓
```

---

## Mode B: Rust-native Bootstrap Elaborator (deferred)

Not on the critical path to Mathlib. Documented here for reference if
bootstrapping from source is ever needed.

### Core Monad Infrastructure

Implement the monad stack in Rust:

```
CoreM    = Environment + MessageLog + Options + NameGenerator
MetaM    = CoreM + MetavarContext + LocalContext + Cache
TermElabM = MetaM + SyntheticMVars + LevelNames
```

### WHNF + Definitional Equality

Port from `Lean/Meta/WHNF.lean` and `Lean/Meta/ExprDefEq.lean`:
- WHNF with transparency modes (reducible, default, all)
- Flex-flex/flex-rigid constraints, higher-order pattern unification
- Eta expansion (lambda and structure), proof irrelevance

### Instance Synthesis

Port from `Lean/Meta/SynthInstance.lean` (~1K lines):
- Tabled resolution, generator/consumer nodes, backtracking

### Term Elaboration

Port from `Lean/Elab/Term.lean`, `App.lean`, `Binders.lean`:
- Implicit argument insertion, application elaboration, coercion

### Pattern Matching

Port from `Lean/Elab/Match.lean` + `Lean/Meta/Match/`:
- Compile patterns to case trees, nested patterns, wildcards

### do-Notation Desugaring

Port from `Lean/Elab/Do.lean`:
- `let x ← e` → bind, `return e` → pure, monadic if/for/while

### Command Elaboration

Port from `Lean/Elab/Command.lean`:
- All declaration kinds, structure/class, inductive, mutual
- namespace/section/open/variable, attribute/set_option, #check/#eval

### Tactics in Bootstrap Mode

**Option A (preferred):** Mark functions `partial`/`unsafe` during bootstrap —
termination proofs not needed for runtime correctness.

**Option B:** Minimal tactic set: `by exact`, `by simp`, `by rfl`, `by omega`,
`by decide`.
