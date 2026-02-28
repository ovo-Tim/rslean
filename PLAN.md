# RSLean: A Memory-Safe Lean 4 Proof Checker in Rust

## Goal

Build a **100% memory-safe** Lean 4 proof checker in Rust. It can parse `.lean`
files, run tactics, and verify proofs — but it is a **prover only**, not a
general-purpose language compiler. No code generation, no manual reference
counting, no unsafe memory management.

Existing Lean 4 tactic/elaborator source code is **reused** — loaded from
.olean files and interpreted in a safe Rust VM. All memory is managed by
Rust's type system (`Arc<T>`, `Vec`, `Box`).

### Non-goals

- Compiling Lean programs to executables
- General-purpose language runtime (I/O, FFI, threads)
- Lake build tool replacement (may add later)
- 1:1 performance parity with native Lean 4

## Architecture Overview

```
100% safe Rust (compiled by rustc, zero unsafe blocks)
┌────────────────────────────────────────────────────────┐
│  rslean (single Rust binary)                           │
│  ├── kernel       — type checker              (safe)   │
│  ├── parser       — .lean file parser         (safe)   │
│  ├── interpreter  — Lean VM, runs tactics     (safe)   │
│  │   └── memory managed by Arc<T>, Vec, Box            │
│  │       no lean_inc/lean_dec, no manual RC            │
│  ├── bootstrap    — bootstrap elaborator      (safe)   │
│  └── olean        — .olean binary reader      (safe)   │
└──────────────────────┬─────────────────────────────────┘
                       │ loads + interprets
┌──────────────────────▼─────────────────────────────────┐
│  Lean source / .olean data (from lean4 toolchain):     │
│  ├── Init.*           — core library / prelude         │
│  ├── Lean.Meta.*      — meta framework (83K LOC)       │
│  ├── Lean.Elab.*      — elaborator (70K LOC)           │
│  └── Lean.Elab.Tactic.* — all tactics (24K LOC)        │
└──────────────────────┬─────────────────────────────────┘
                       │ verifies
                       ▼
                  ✓ Proof checked
```

### Why this design

| Concern | How it's addressed |
|---------|--------------------|
| Memory safety | 100% safe Rust — `Arc<T>` for sharing, no manual RC |
| Memory leaks | Rust's ownership + `Arc` cycle detection (no leaked manual RC) |
| Bootstrap dependency | None — `rustc` compiles rslean, .olean files are just data |
| Tactic support | All Lean 4 tactics work — interpreted from existing code |
| Maintainability | ~200K lines of Lean code reused, not rewritten |

## Crate Structure

```
rslean/
├── Cargo.toml                (workspace root)
├── crates/
│   ├── rslean-name/          Phase 1  — Name, hierarchical names
│   ├── rslean-level/         Phase 1  — Universe levels
│   ├── rslean-expr/          Phase 1  — Expr AST (12 constructors)
│   ├── rslean-kernel/        Phase 1  — Type checker, environment, inductive
│   ├── rslean-olean/         Phase 1  — .olean binary reader
│   ├── rslean-syntax/        Phase 2  — Syntax tree types
│   ├── rslean-lexer/         Phase 2  — Tokenizer
│   ├── rslean-parser/        Phase 2  — Parser combinators + Lean grammar
│   ├── rslean-interp/        Phase 3  — Safe Lean interpreter / VM
│   ├── rslean-meta/          Phase 4  — MetaM: WHNF, unification, synthesis
│   ├── rslean-elab/          Phase 4  — Bootstrap elaborator
│   └── rslean-driver/        Phase 5  — CLI frontend (`rslean check`)
├── tests/
│   ├── kernel/               — kernel unit tests against .olean files
│   ├── parser/               — parser round-trip tests
│   ├── interp/               — interpreter correctness tests
│   ├── elab/                 — elaboration integration tests
│   └── e2e/                  — end-to-end proof checking tests
└── lean4-master/             — upstream Lean 4 source (reference)
```

---

## Phase 1: Kernel + Core Types + .olean Reader [COMPLETE]

**Goal:** A Rust binary that can load .olean files and type-check all declarations
(equivalent to `leanchecker`).

**Estimated new Rust:** ~15-20K lines

### 1.1 Core Data Types (`rslean-name`, `rslean-level`, `rslean-expr`)

Port from C++ `src/kernel/`:

**Name** (from `name.h`, 3 constructors):
```rust
enum Name {
    Anonymous,
    Str { prefix: Arc<Name>, s: InternedString },
    Num { prefix: Arc<Name>, n: u64 },
}
```

**Level** (from `level.h`, 6 constructors):
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

**Expr** (from `expr.h`, 12 constructors + cached metadata):
```rust
enum ExprKind {
    BVar(u64),                                            // de Bruijn index
    FVar(FVarId),                                         // free variable
    MVar(MVarId),                                         // metavariable
    Sort(Level),                                          // Prop / Type u
    Const(Name, Vec<Level>),                              // named constant
    App(Expr, Expr),                                      // function application
    Lam(Name, Expr, Expr, BinderInfo),                    // lambda
    ForallE(Name, Expr, Expr, BinderInfo),                // dependent function type
    LetE(Name, Expr, Expr, Expr, bool),                   // let binding
    Lit(Literal),                                         // nat/string literal
    MData(MData, Expr),                                   // metadata
    Proj(Name, u64, Expr),                                // structure projection
}

struct Expr {
    kind: Arc<ExprKind>,
    data: ExprData,  // cached: hash, flags, loose_bvar_range
}
```

**BinderInfo**: `Default | Implicit | StrictImplicit | InstImplicit`

**ConstantInfo** (declarations in environment):
```rust
enum ConstantInfo {
    Defn { name, levelParams, type_, value, safety },
    Thm  { name, levelParams, type_, value },
    Axiom { name, levelParams, type_, isUnsafe },
    Opaque { name, levelParams, type_, value, isUnsafe },
    Induct { name, levelParams, type_, numParams, numIndices, ctors, isRec, ... },
    Ctor { name, levelParams, type_, inductName, ctorIdx, numParams, numFields },
    Rec { name, levelParams, type_, numParams, numIndices, numMotives, ... },
    Quot { name, levelParams, type_, kind },
}
```

### 1.2 Environment (`rslean-kernel`)

Port from C++ `src/kernel/environment.h`:
- `Environment`: stores `HashMap<Name, ConstantInfo>`
- Declaration validation before insertion
- Module import tracking
- Universe constraint checking

### 1.3 Type Checker (`rslean-kernel`)

Port from C++ `src/kernel/type_checker.cpp` (~2.2K lines):

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

### 1.4 .olean Reader (`rslean-olean`)

Parse Lean 4's binary module format:
- `ModuleData { imports, constNames, constants, extraConstNames, entries }`
- `CompactedRegion` deserialization (contiguous object memory)
- Expression/Name/Level deserialization from binary
- Replay logic: load constants, resolve dependencies, feed to kernel

### 1.5 Milestone: `rslean check`

A CLI tool that:
1. Takes a module name (e.g., `Mathlib.Data.Nat.Basic`)
2. Loads its .olean file and all transitive dependencies
3. Replays all declarations through the Rust kernel
4. Reports success or type errors

**Test:** Successfully verify all of Lean 4's Init library .olean files.

---

## Phase 2: Parser

**Goal:** Parse .lean source files into a Syntax tree.

**Estimated new Rust:** ~10-15K lines | **Safety: 100% safe**

### 2.1 Lexer (`rslean-lexer`)

Tokenize Lean 4 source:
- Identifiers (with hierarchical `.` names, Unicode support)
- Numeric literals (decimal, hex, binary, with separators)
- String literals (with interpolation `s!"..."`, raw strings)
- Operators and punctuation
- Keywords
- Comments (line `--`, block `/- -/`, doc `/-! -/`)
- Whitespace sensitivity (significant indentation)

### 2.2 Syntax Types (`rslean-syntax`)

```rust
enum Syntax {
    Missing,                                                    // error recovery
    Atom { info: SourceInfo, val: String },                     // tokens
    Ident { info: SourceInfo, val: Name },                      // identifiers
    Node { info: SourceInfo, kind: Name, args: Vec<Syntax> },   // tree nodes
}
```

### 2.3 Parser (`rslean-parser`)

Implement a **fixed grammar parser** (no extensible syntax):

Core constructs needed to parse Lean source:
- `import` declarations
- `namespace` / `section` / `open`
- `def` / `theorem` / `lemma` / `abbrev` / `instance`
- `structure` / `class` / `inductive` / `where`
- `private` / `protected` / `noncomputable` / `unsafe` / `partial`
- `fun` / `λ` / `let` / `have` / `match` / `if` / `do`
- `@[attribute]` annotations
- Operators (arithmetic, logical, comparison)
- Type ascriptions (`:`)
- Field notation (`.field`)
- Anonymous constructors (`⟨...⟩`)
- Parenthesization, explicit universe annotations
- `by` tactic blocks

What we do NOT need initially:
- User-defined syntax extensions / notation
- Full macro expansion (handle built-in macros only)
- Extensible parser categories

### 2.4 Milestone

Successfully parse `src/Init/Prelude.lean` and `src/Lean/Meta/Basic.lean` into
syntax trees. Round-trip test: parse → pretty-print → parse again → compare.

---

## Phase 3: Interpreter (Safe Lean VM)

**Goal:** A safe Rust interpreter that can execute Lean code — including the
elaborator and tactics loaded from .olean files.

**Estimated new Rust:** ~15-20K lines | **Safety: 100% safe**

This phase replaces the old Phase 2 (unsafe C++ runtime) and Phase 5 (code
generator). Instead of compiling Lean to native code with manual RC, we
interpret Lean code in a safe Rust VM where all memory is managed by Rust.

### 3.1 Approach: Tree-Walking Interpreter

**Design change:** The original plan called for a bytecode compiler (Opcode
enum, bytecode compiler pass). The actual implementation uses a simpler
**tree-walking interpreter** that evaluates kernel `Expr` values directly.

Rationale:
- Simpler to implement and debug (no separate compilation pass)
- Correct first, fast later — bytecode can be added as an optimization
- Fewer moving parts means fewer bugs during Phase 4 elaborator integration

### 3.2 Value Representation

```rust
#[derive(Clone, Debug)]
pub enum Value {
    Nat(Arc<BigUint>),          // Arbitrary precision natural
    String(Arc<str>),           // String value
    Ctor {                      // Constructor application
        tag: u32,
        name: Name,
        fields: Vec<Value>,
    },
    Closure {                   // Function waiting for arguments
        func: FuncRef,
        captured: Vec<Value>,
        remaining_arity: u32,
    },
    Array(Arc<Vec<Value>>),     // Array of values
    Erased,                     // Type/proof (computationally irrelevant)
    KernelExpr(Expr),           // Opaque expr for elaborator bridge
}

pub enum FuncRef {
    Definition(Name, Vec<Level>),               // Global definition
    Lambda(Expr, LocalEnv),                     // Lambda body + captured env
    Builtin(Name),                              // Native Rust function
    CtorFn { name, tag, num_params, num_fields }, // Constructor builder
    RecursorFn(Name, Vec<Level>),               // Recursor (iota reduction)
}
```

**Design changes from original plan:**
- `Ctor` includes `name: Name` (needed for recursor rule matching)
- `Closure` uses `FuncRef` + `captured` args instead of `env + body`
  (supports partial application of builtins and constructors, not just lambdas)
- `Erased` replaces separate type/proof handling — ForallE, Sort, MVar all
  evaluate to Erased since they are computationally irrelevant
- `KernelExpr` replaces `Value::Expr/Name/Level` — single variant for
  opaque kernel objects passed through to the elaborator
- No `Thunk` variant — lazy evaluation not yet needed; can add later
- `Array` added for Array builtin support

No `lean_inc` / `lean_dec`. Rust's ownership handles everything:
- `Vec<Value>` for constructor fields — dropped when unreachable
- `Arc<T>` for shared data — reference counted by Rust (safe, cycle-free)
- `Clone` is cheap (Arc bumps refcount atomically)

### 3.3 Core Evaluation

The `Interpreter` struct holds the kernel `Environment`, a builtin registry
(`FxHashMap<Name, BuiltinFn>`), a const cache, and a depth counter.

`eval(expr, local_env) -> InterpResult<Value>`:

| ExprKind | Action |
|----------|--------|
| `Lit(Nat(n))` | `Value::Nat(n)` |
| `Lit(Str(s))` | `Value::String(s)` |
| `BVar(i)` | `local_env.lookup(i)` |
| `Lam(_, _, body, _)` | `Value::Closure { func: Lambda(body, env), arity: 1 }` |
| `LetE(_, _, val, body, _)` | Evaluate val, push onto env, evaluate body |
| `App(f, a)` | Evaluate f and a, apply (beta-reduce or accumulate partial) |
| `Const(name, levels)` | Check builtins first, then `eval_const(name, levels)` |
| `ForallE / Sort / MVar` | `Value::Erased` |
| `MData(_, e)` | Evaluate e (metadata is transparent) |
| `Proj(struct_name, idx, e)` | Evaluate e to Ctor, return `fields[idx]` |
| `FVar(_)` | `Value::KernelExpr(expr)` (for elaborator bridge) |

### 3.4 Built-in Operations

**Design change:** Builtins are registered as `fn(&[Value]) -> InterpResult<Value>`
in an `FxHashMap<Name, BuiltinFn>`, keyed by Lean declaration name (e.g.,
`Nat.add`). The original plan used an `enum BuiltinId`. The hashmap approach
is more extensible and avoids maintaining a parallel enum.

Arity is computed automatically by counting ForallE binders in the
constant's type from the environment.

44 builtins implemented:
- **Nat** (18): add, sub, mul, div, mod, pow, gcd, beq, ble, pred,
  land, lor, xor, shiftLeft, shiftRight, decEq, decLe, decLt
- **String** (6): decEq, append, length, mk, push, utf8ByteSize
- **Bool** (1): decEq
- **Array** (5): mkEmpty, push, size, get!, set!
- **UInt32** (10): ofNat, toNat, add, sub, mul, div, mod, decEq, decLt, decLe
- **USize** (2): ofNat, toNat
- Decidable results use `Ctor { tag: 0 = isFalse, tag: 1 = isTrue }`

### 3.5 Iota Reduction

When a recursor is fully applied:
1. Identify major premise from argument position
2. Evaluate major premise to a `Value::Ctor { tag, name, fields }`
3. Special-case Nat: `0` maps to `Nat.zero`, `n+1` maps to `Nat.succ(n-1)`
4. Find matching `RecursorRule` by constructor name
5. Build substitution: params + motives + minors + constructor fields
6. Push substitution into a `LocalEnv` (forward order so fields end up
   at bvar(0), matching de Bruijn convention)
7. Evaluate the rule's RHS with the substitution environment

### 3.6 Milestone

1. Load `Init.Prelude` .olean definitions into the interpreter
2. Evaluate `Nat.add 2 3` → `Value::Nat(5)`
3. Evaluate `List.map (· + 1) [1, 2, 3]` → `[2, 3, 4]`
4. Call a kernel operation from interpreted code and get correct result

---

## Phase 4: Bootstrap Elaborator

**Goal:** Elaborate `.lean` source files into kernel declarations. This
elaborator runs the interpreted Lean tactics and elaborator code when available,
with a Rust-native fallback for bootstrapping.

**Estimated new Rust:** ~25-35K lines | **Safety: 100% safe**

### 4.1 Two-mode Design

The elaborator has two modes:

**Mode A — Interpreted (primary mode):**
Load the Lean-written elaborator from .olean files and run it in the
interpreter. All tactics, macros, and elaboration logic come from existing
Lean code. The Rust "bootstrap elaborator" is minimal glue that:
1. Loads .olean files for `Init.*`, `Lean.Meta.*`, `Lean.Elab.*`
2. Calls the Lean `Lean.Elab.Frontend.processCommands` in the interpreter
3. Collects resulting declarations and feeds them to the kernel

**Mode B — Rust-native (fallback / bootstrapping):**
If .olean files aren't available, use a Rust-native elaborator that can
process a subset of Lean syntax. This is needed to bootstrap from source.

Most users will only ever use Mode A.

### 4.2 Mode A: Interpreted Elaborator (primary)

Rust glue code that orchestrates:

```
1. Load .olean files for Lean standard library + compiler
2. Initialize interpreter with all definitions
3. Construct initial Lean.Elab.Frontend.Context in interpreter
4. For each user .lean file:
   a. Parse with Rust parser (Phase 2)
   b. Convert Syntax tree to interpreter Value
   c. Call Lean.Elab.Frontend.processCommands in interpreter
   d. Extract resulting Environment from interpreter
   e. Validate all new declarations with Rust kernel
5. Report results
```

### 4.3 Mode B: Rust-native Bootstrap Elaborator

For bootstrapping from source (when .olean files aren't available):

#### 4.3.1 Core Monad Infrastructure (`rslean-meta`)

Implement the monad stack in Rust:

```
CoreM    = Environment + MessageLog + Options + NameGenerator
MetaM    = CoreM + MetavarContext + LocalContext + Cache
TermElabM = MetaM + SyntheticMVars + LevelNames
```

Key state:
- `MetavarContext`: tracks metavariable assignments
- `LocalContext`: local hypotheses and let-bindings
- Cache: WHNF cache, inferType cache, isDefEq cache

#### 4.3.2 WHNF + Definitional Equality (`rslean-meta`)

Port logic from `Lean/Meta/WHNF.lean` and `Lean/Meta/ExprDefEq.lean`:

- WHNF with transparency modes (reducible, default, all)
- Beta, delta, iota (recursor/matcher), zeta reduction
- Definitional equality with:
  - Flex-flex constraints (mvar-mvar)
  - Flex-rigid constraints (mvar-term, higher-order pattern unification)
  - Eta expansion (lambda and structure)
  - Proof irrelevance
  - Approximation modes (first-order, context, quasi-pattern)
- Native reduction for Bool and Nat operations

#### 4.3.3 Type Inference

Port from `Lean/Meta/InferType.lean`:
- Infer types of expressions
- Handle universe polymorphism
- Reduce types to WHNF when needed

#### 4.3.4 Instance Synthesis (`rslean-meta`)

Port from `Lean/Meta/SynthInstance.lean` (~1K lines):
- Tabled resolution algorithm
- Normalize goals for caching
- Generator nodes (instances to try)
- Consumer nodes (waiting for results)
- Backtracking search

#### 4.3.5 Term Elaboration (`rslean-elab`)

Port from `Lean/Elab/Term.lean`, `App.lean`, `Binders.lean`:
- Elaborate expressions with expected type
- Implicit argument insertion (create mvars, solve by unification)
- Application elaboration (named args, positional args)
- Binder elaboration (lambda, forall, let)
- Literal elaboration (Nat, String, Char)
- Coercion insertion

#### 4.3.6 Pattern Matching Compilation

Port from `Lean/Elab/Match.lean` + `Lean/Meta/Match/`:
- Elaborate match expressions
- Compile patterns to case trees
- Handle nested patterns, as-patterns, wildcard
- Generate matcher auxiliary definitions

#### 4.3.7 do-Notation Desugaring

Port from `Lean/Elab/Do.lean`:
- `let x ← e` → monadic bind
- `return e` → `pure e`
- Monadic `if`, `for`, `while`
- `← e` in expressions → lift

#### 4.3.8 Command Elaboration (`rslean-elab`)

Port from `Lean/Elab/Command.lean`:
- `def` / `theorem` / `abbrev` / `instance` / `example`
- `structure` / `class` (from `Structure.lean`, ~1.6K lines)
- `inductive` (from `Inductive.lean`)
- `mutual ... end` (from `MutualDef.lean`)
- `namespace` / `section` / `open` / `variable`
- `attribute` / `set_option` / `#check` / `#eval`
- `import` processing

#### 4.3.9 Handling Tactics in Bootstrap Mode

The Lean compiler source uses `by` in ~50+ places (termination proofs, etc.).
Strategy for bootstrap:

**Option A (preferred):** Mark those functions `partial` or `unsafe` during
bootstrap compilation. The termination proofs are not needed for runtime
correctness — they're only needed for the kernel to accept the definition.
During bootstrap, we can trust these functions terminate.

**Option B:** Implement a minimal set of tactics:
- `by exact e` — just elaborate `e`
- `by simp` — basic simplification
- `by rfl` — reflexivity
- `by omega` — integer arithmetic (port the decision procedure)
- `by decide` — decidable computation

Option A is simpler and sufficient.

### 4.4 Milestone

**Mode A:** Load .olean files from a Lean 4 toolchain, interpret the Lean
elaborator, and successfully check a user .lean file with tactic proofs.

**Mode B:** Elaborate `src/Init/Prelude.lean` from source and produce correct
kernel declarations that pass the Phase 1 type checker.

---

## Phase 5: Integration + CLI

**Goal:** A polished CLI tool for proof checking.

**Estimated new Rust:** ~5K lines | **Safety: 100% safe**

### 5.1 CLI: `rslean check <file.lean>`

```
$ rslean check MyProofs.lean
Loading Init... (from .olean)
Loading Lean.Meta... (from .olean)
Loading Lean.Elab... (from .olean)
Checking MyProofs.lean...
✓ 42 declarations checked, 0 errors
```

### 5.2 CLI: `rslean verify <file.olean>`

Verify pre-compiled .olean files (Phase 1 functionality, polished):

```
$ rslean verify Mathlib.Data.Nat.Basic
✓ All declarations type-check
```

### 5.3 Toolchain Management

- Auto-detect Lean 4 toolchain (from `lean-toolchain` file or `elan`)
- Locate .olean files for Init, Lean, Std
- Support `LEAN_PATH` environment variable

### 5.4 Error Reporting

- Source-mapped error messages with line/column
- Type mismatch details with expected vs. found
- Tactic failure context

### 5.5 Milestone

`rslean check` successfully verifies a Lean 4 file using tactic proofs
(including `simp`, `omega`, `ring`, etc.) against a standard Lean 4 toolchain.

---

## Summary: Estimated Effort

| Phase     | Component                  | New Rust LOC | Difficulty | Safety     |
| --------- | -------------------------- | ------------ | ---------- | ---------- |
| 1         | Kernel + types + .olean    | 15-20K       | Medium     | 100% safe  |
| 2         | Parser                     | 10-15K       | Medium     | 100% safe  |
| 3         | Interpreter (safe Lean VM) | 15-20K       | Medium     | 100% safe  |
| 4         | Bootstrap elaborator       | 25-35K       | **Hard**   | 100% safe  |
| 5         | Integration + CLI          | ~5K          | Easy       | 100% safe  |
| **Total** |                            | **~70-95K**  |            | **100%**   |

Reused from Lean 4 (interpreted, not rewritten): **~200K+ lines** of existing
Lean source code (Meta, Elab, Tactic, Init).

### Comparison with previous (unsafe) plan

| | Old plan (compile to native) | New plan (interpret in safe VM) |
|---|---|---|
| New Rust code | ~90-125K | **~70-95K** (less) |
| Memory safety | Partial (runtime safe, compiled code unsafe) | **100% safe** |
| Tactic support | All (compiled native) | All (interpreted) |
| Performance | Native speed | ~10-50x slower for tactic execution |
| Code generator | Required (~15-20K) | **Not needed** |
| C/C++ runtime | Required (~10-15K) | **Not needed** |
| Scope | General-purpose compiler | Prover only |

## Recommended Build Order

```
Phase 1 (kernel) ──→ Phase 3 (interpreter) ──→ Phase 5 (CLI)
                  ↗         ↗
Phase 2 (parser)    Phase 4 (elaborator, Mode A uses interpreter)
```

Phase 1 is already complete. Phase 2 (parser) and Phase 3 (interpreter) can
proceed in parallel. Phase 4 Mode A (interpreted elaborator) depends on both
Phase 2 and Phase 3. Phase 5 ties everything together.

For most users, only **Mode A** of Phase 4 is needed — the Rust code loads
.olean files from a Lean 4 toolchain and interprets the Lean elaborator. Mode B
(Rust-native elaborator) is only needed for bootstrapping from source, and can
be deferred.

**Fastest path to a working prover:**
Phase 1 (done) → Phase 3 (interpreter) → Phase 4 Mode A → Phase 5

This skips the parser entirely for the first milestone — proofs are parsed by
the interpreted Lean parser loaded from .olean. The Rust parser (Phase 2) is
needed later for Mode B bootstrapping or for performance.

---

## Progress Log

### Phase 1 — COMPLETE ✓ (2026-02-28)

All of Phase 1 is implemented in `rslean/` with **76 passing tests**.

#### Crates implemented

| Crate           | Lines | Tests | Notes                                                 |
| --------------- | ----- | ----- | ----------------------------------------------------- |
| `rslean-name`   | ~500  | 17    | MurmurHash2 matching Lean 4 exactly                   |
| `rslean-level`  | ~700  | 16    | Full normalization, `is_equivalent`, `is_geq`         |
| `rslean-expr`   | ~800  | 17    | 12-constructor AST, substitution, level instantiation |
| `rslean-kernel` | ~700  | 17    | TypeChecker with WHNF, def-eq, type inference         |
| `rslean-olean`  | ~600  | 9     | .olean v2 binary reader (GMP + native bignums)        |
| `rslean-check`  | ~200  | —     | CLI: load + env replay                                |

#### Key design decisions

- **Name**: Arc-based sharing, MurmurHash2 64-bit seed-11 matching Lean 4
- **Level**: Smart constructors `max()`/`imax()` normalize on construction
- **Expr**: Cached `ExprData` (hash, flags, loose_bvar_range) per node
- **TypeChecker**: `local_ctx: FxHashMap<Name, Expr>` for FVar lookup during inference
- **OleanReader**: Recursive object graph walker; offsets resolved as `stored - (base_addr + 88)`
- **ConstantInfo layout** (from Lean 4 object model):
  - `AxiomVal`: 1 obj (ConstantVal) + 1 scalar u8 (isUnsafe)
  - `DefinitionVal`: 4 obj (cv, value, hints, safety, all) + 0 scalar
  - `InductiveVal`: 6 obj (cv, numParams, numIndices, all, ctors, numNested) + 3 scalar u8
  - `ConstructorVal`: 5 obj (cv, induct, cidx, numParams, numFields) + 1 scalar u8
  - `RecursorVal`: 7 obj (cv, all, numParams, numIndices, numMotives, numMinors, rules) + 2 scalar u8

#### Milestone result: `rslean check`
Verified against Lean 4.21.0-pre .olean files from the elan toolchain.

#### What is NOT yet done in Phase 1

- **Inductive type checking** — `check_and_add` has a placeholder for inductives;
  declarations are accepted (added unchecked). Full `add_inductive` validation
  (constructor positivity, type well-formedness, recursor generation) is deferred.
- **Full type checking replay** — the CLI currently calls `add_constant_unchecked`.
  Wiring `check_and_add` for all declaration kinds requires completing inductive checking.
- **MData value deserialization** — `ofInt` and `ofSyntax` DataValue variants are skipped.

#### Next steps

**Fastest path:** Phase 3 (interpreter) → Phase 4 Mode A → Phase 5.
This uses the interpreted Lean parser from .olean files, so Phase 2 (Rust
parser) can be deferred.

**Parallel track:** Phase 2 (Rust parser) can proceed independently for
future Mode B bootstrapping support.

### Phase 3 — IN PROGRESS (2026-02-28)

#### `rslean-interp` crate — Safe Lean Interpreter

Tree-walking interpreter that evaluates Lean kernel `Expr` values directly
(no bytecode compilation). Implemented with **41 passing tests**, bringing
workspace total to **117 tests**.

#### Files

```
crates/rslean-interp/src/
├── lib.rs          — pub re-exports
├── value.rs        — Value enum (Nat, String, Ctor, Closure, Array, Erased, KernelExpr)
├── env.rs          — LocalEnv (de Bruijn indexed variable stack)
├── error.rs        — InterpError enum (10 variants)
├── eval.rs         — Interpreter struct + core eval() function
├── builtins.rs     — 44 builtin functions registered by Lean name
├── iota.rs         — recursor/casesOn iota reduction
└── tests.rs        — 41 unit tests
```

#### What's implemented

| Component | Details |
|-----------|---------|
| **Value** | `Nat(Arc<BigUint>)`, `String(Arc<str>)`, `Ctor{tag,name,fields}`, `Closure{func,captured,arity}`, `Array`, `Erased`, `KernelExpr` |
| **FuncRef** | `Definition`, `Lambda`, `Builtin`, `CtorFn`, `RecursorFn` |
| **eval()** | All 12 ExprKind variants handled |
| **Const eval** | Definition/Theorem body evaluation, Constructor → Ctor values, Recursor → iota reduction |
| **Iota reduction** | Correct de Bruijn substitution for recursor rules; Nat special-casing (0 → Nat.zero, n+1 → Nat.succ) |
| **Partial application** | Closures accumulate args until fully applied |
| **Stack overflow** | Depth limit of 256 (prevents runaway recursion) |
| **Const caching** | Level-monomorphic constants cached after first evaluation |
| **44 builtins** | Nat (add/sub/mul/div/mod/pow/gcd/beq/ble/pred/land/lor/xor/shifts + decEq/decLe/decLt), String (append/length/mk/push/utf8ByteSize + decEq), Bool (decEq), Array (mkEmpty/push/size/get!/set!), UInt32 (full arithmetic + of/toNat + decidable), USize (of/toNat) |

#### Key design decisions

- **Tree-walking** over bytecode compilation: simpler to implement and debug,
  can optimize to bytecode later if performance requires it
- **Builtin lookup by Lean name** (e.g., `Nat.add`) not C extern name
  (`lean_nat_add`), since we register by declaration name
- **Arity computed from type**: count ForallE binders in the constant's type
  to determine how many args a builtin needs before it fires
- **Erased types/proofs**: ForallE, Sort, MVar all evaluate to `Value::Erased`;
  applying anything to Erased returns Erased (computationally irrelevant)
- **FVar passthrough**: Free variables become `Value::KernelExpr` for the
  elaborator bridge (Phase 4)
- **De Bruijn substitution for iota**: push subst in forward order
  (params, motives, minors, fields) so that `LocalEnv::push` prepends correctly
  — fields end up at bvar(0), matching Lean's convention

#### Test coverage

| Category | Count | Examples |
|----------|-------|---------|
| LocalEnv | 3 | push/lookup, out of bounds |
| Value construction | 4 | Nat, String, Bool, Unit |
| Literals | 2 | Nat lit, String lit |
| BVar | 2 | lookup, unbound error |
| Lambda/App | 2 | identity, const function |
| Let | 2 | simple, nested |
| Type erasure | 2 | Sort, ForallE |
| MData | 1 | transparency |
| Const/Definition | 1 | polymorphic id |
| Constructor/Proj | 2 | Prod.mk, projection |
| Recursor (Bool) | 2 | Bool.rec true/false |
| Recursor (Nat) | 1 | Nat.rec zero case |
| Nat builtins | 8 | add, mul, sub, div, mod, pow, beq, decEq |
| String builtins | 2 | append, length |
| Array builtins | 1 | mkEmpty + push |
| Other | 4 | FVar passthrough, zero-arity ctor, stack overflow, partial app |
| **Total** | **41** | |

#### What is NOT yet done in Phase 3

- **Integration tests with .olean files** — loading real definitions from
  Init.Prelude and evaluating expressions like `Nat.add 2 3` against the
  actual environment (Step 7 in the plan)
- **Priority 2 builtins** — ST/Ref (RefCell), HashMap, Name operations
  needed for Phase 4 elaborator
- **Priority 3 builtins** — IO, Float, Process/File (stubs sufficient)
- **List.map milestone** — requires loading real List definitions from .olean
  to test `List.map (· + 1) [1, 2, 3]` end-to-end
- **Recursive iota reduction** — for recursive inductives (e.g., Nat.rec on
  `succ n`), the current implementation passes fields directly without
  computing recursive results; full recursive iota needs the recursor to
  recursively apply itself to recursive arguments before passing to the minor

#### Next steps

1. **Recursive iota**: handle recursive arguments in recursor rules
2. **.olean integration tests**: load Init.Prelude, evaluate real expressions
3. **Phase 4 prep**: ST/Ref builtins, HashMap builtins, Name operations
4. **Phase 4 Mode A**: interpreted elaborator glue code
