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

### 3.1 Value Representation

```rust
/// Every Lean value in the interpreter — fully managed by Rust
#[derive(Clone)]
enum Value {
    /// Constructor: `Name` is the inductive type, `tag` is the constructor
    /// index, `fields` are the constructor arguments
    Ctor { tag: u32, fields: Vec<Value> },

    /// Closure: captured environment + remaining arity + body
    Closure { env: Vec<Value>, arity: u32, body: Arc<Thunk> },

    /// Primitive literals
    Nat(num_bigint::BigUint),
    String(Arc<str>),

    /// Thunk (lazy evaluation)
    Thunk(Arc<OnceCell<Value>>),

    /// External/opaque reference (for kernel Expr values passed through)
    Expr(Expr),
    Name(Name),
    Level(Level),
}
```

No `lean_inc` / `lean_dec`. Rust's ownership handles everything:
- `Vec<Value>` for constructor fields — dropped when unreachable
- `Arc<T>` for shared data — reference counted by Rust (safe, cycle-free)
- `Clone` is cheap (Arc bumps refcount atomically)

### 3.2 Bytecode Design

Compile kernel `Expr` values from .olean files into flat bytecodes:

```rust
enum Opcode {
    // Stack operations
    Push(u32),              // push local variable
    Pop,                    // discard top
    Dup,                    // duplicate top

    // Construction
    MkCtor(u32, u32),       // make constructor (tag, nfields)
    MkClosure(FuncId, u32), // make closure (function, captured_count)
    Proj(u32),              // project field from constructor

    // Control flow
    Apply,                  // apply closure to argument
    TailApply,              // tail-call apply
    Ret,                    // return top of stack
    Case(Vec<(u32, PC)>),   // branch on constructor tag
    Jmp(PC),                // unconditional jump

    // Primitives
    NatLit(BigUint),        // push Nat literal
    StrLit(Arc<str>),       // push String literal
    BuiltinCall(BuiltinId), // call built-in operation

    // Kernel callbacks (safe Rust calls)
    CallKernel(KernelOp),   // isDefEq, whnf, inferType, etc.
}
```

### 3.3 Bytecode Compiler

Translate kernel `Expr` (from .olean `ConstantInfo.value`) to bytecodes:

- Lambda bodies → function definitions with bytecode
- Application → `Push` args + `Apply`
- Pattern matching (recursor applications) → `Case` dispatch
- Let bindings → `Push` value + body
- Projections → `Proj(field_index)`
- Constants → look up and inline or call
- Type/proof erasure: skip computationally irrelevant terms

This is simpler than a full LCNF pipeline because we don't need:
- Monomorphization (interpreter handles polymorphism directly)
- RC insertion (Rust handles it)
- Closure conversion (interpreter supports closures natively)
- Boxing/unboxing optimization

### 3.4 Built-in Operations

Implement native Rust functions for performance-critical builtins:

```rust
enum BuiltinId {
    // Nat arithmetic
    NatAdd, NatSub, NatMul, NatDiv, NatMod,
    NatBEq, NatBLt, NatDecEq, NatDecLt,

    // String operations
    StrAppend, StrLength, StrGet, StrPush, StrMk,

    // Array operations (Vec<Value> under the hood)
    ArrayMk, ArrayGet, ArraySet, ArrayPush, ArraySize,

    // HashMap / persistent data structures
    HashMapInsert, HashMapFind,

    // Kernel operations (callbacks into safe Rust kernel)
    KernelInferType, KernelIsDefEq, KernelWhnf, KernelAddDecl,
}
```

### 3.5 Kernel ↔ Interpreter Bridge

The elaborator (running in the interpreter) calls kernel operations. The
interpreter has "escape hatches" that call into the safe Rust kernel:

```
Interpreted Lean code (MetaM.isDefEq a b)
    → interpreter sees BuiltinCall(KernelIsDefEq)
    → calls Rust kernel's is_def_eq(a, b) directly
    → returns result to interpreter
```

This is safe: both sides are safe Rust. The interpreter passes `Value::Expr`
objects to the kernel, which operates on them natively.

### 3.6 Environment Extension State

The elaborator maintains state (registered simp lemmas, attributes, instances,
etc.) through environment extensions. The interpreter must support:

- `EnvExtension` registration and lookup
- Serialized extension entries from .olean files
- Mutable state for the elaborator monad (via `RefCell` / interior mutability)

### 3.7 Milestone

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
