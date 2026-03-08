# RSLean: A Memory-Safe Lean 4 Proof Checker in Rust

## Goal

Build a **100% memory-safe** Lean 4 proof checker in Rust. It can parse `.lean`
files, run tactics, and verify proofs — but it is a **prover only**, not a
general-purpose language compiler. No code generation, no manual reference
counting, no unsafe memory management.

Existing Lean 4 tactic/elaborator source code is **reused** — loaded from
.olean files and interpreted in a safe Rust VM. All memory is managed by
Rust's type system (`Arc<T>`, `Vec`, `Box`).

**Target: verify Mathlib proofs.**

### Non-goals

- Compiling Lean programs to executables
- General-purpose language runtime (I/O, FFI, threads)
- Lake build tool replacement
- 1:1 performance parity with native Lean 4

## Architecture

```
100% safe Rust (zero unsafe blocks)
┌────────────────────────────────────────────────────────┐
│  rslean                                                │
│  ├── rslean-kernel    — type checker, environment      │
│  ├── rslean-olean     — .olean binary reader           │
│  ├── rslean-interp    — safe Lean VM (tree-walking)    │
│  │   ├── 272 builtins (Nat, String, Array, IO, ...)    │
│  │   ├── loader (multi-module .olean loading)          │
│  │   └── elaboration bridge (process_lean_input)       │
│  ├── rslean-parser    — .lean source → Syntax trees    │
│  ├── rslean-lexer     — tokenizer                      │
│  ├── rslean-syntax    — Syntax tree types               │
│  ├── rslean-check     — CLI frontend                   │
│  └── rslean-{name,level,expr} — core data types        │
└──────────────────────┬─────────────────────────────────┘
                       │ loads + interprets
┌──────────────────────▼─────────────────────────────────┐
│  .olean data (from lean4 toolchain):                   │
│  ├── Init.*              — core library / prelude      │
│  ├── Lean.Parser.*       — parser (used via interp)    │
│  ├── Lean.Meta.*         — meta framework              │
│  ├── Lean.Elab.*         — elaborator + tactics        │
│  └── Std.* / Mathlib.*   — user libraries              │
└──────────────────────┬─────────────────────────────────┘
                       │ verifies
                       ▼
                  ✓ Proof checked
```

**Key insight:** The elaboration pipeline uses Lean's own parser and elaborator
loaded from .olean files, interpreted by our Rust VM. The Rust parser (Phase 2)
is available for bootstrapping from source but is not on the critical path to
Mathlib verification.

## Crate Structure

```
rslean/
├── Cargo.toml                (workspace root, 10 crates)
├── crates/
│   ├── rslean-name/          — Name, hierarchical names          (~670 lines)
│   ├── rslean-level/         — Universe levels                   (~990 lines)
│   ├── rslean-expr/          — Expr AST (12 constructors)        (~1700 lines)
│   ├── rslean-kernel/        — Type checker, environment         (~1480 lines)
│   ├── rslean-olean/         — .olean binary reader              (~1620 lines)
│   ├── rslean-syntax/        — Syntax tree types                 (~730 lines)
│   ├── rslean-lexer/         — Tokenizer                         (~1580 lines)
│   ├── rslean-parser/        — Recursive descent parser          (~3540 lines)
│   ├── rslean-interp/        — Safe Lean VM + elaboration bridge (~10300 lines)
│   └── rslean-check/         — CLI tool                          (~420 lines)
├── lean4-master/             — upstream Lean 4 source (reference)
└── PLAN.md
```

Total: ~23K lines Rust, 231 tests passing.

---

## What's Done

### Phase 1: Kernel + Core Types + .olean Reader ✓

Rust binary that loads .olean files and type-checks declarations.

| Crate           | Lines | Tests | What it does                                          |
| --------------- | ----- | ----- | ----------------------------------------------------- |
| `rslean-name`   | ~670  | 17    | Name type, MurmurHash2 matching Lean 4                |
| `rslean-level`  | ~990  | 16    | Universe levels, normalization, `is_equivalent`       |
| `rslean-expr`   | ~1700 | 17    | 12-constructor AST, substitution, caching             |
| `rslean-kernel` | ~1480 | 17    | TypeChecker: WHNF, def-eq, type inference             |
| `rslean-olean`  | ~1620 | 9     | .olean v2 binary reader (GMP + native bignums)        |
| `rslean-check`  | ~420  | —     | CLI: .olean verification + .lean elaboration          |

**Known gaps:**
- Inductive type checking has a placeholder (`add_constant_unchecked`)
- MData `ofInt`/`ofSyntax` DataValue variants skipped

### Phase 2: Parser ✓

Hand-written recursive descent parser with Pratt parsing for operators.
Parses `.lean` source files into `Syntax` trees.

| Crate            | Lines | Tests | What it does                                         |
| ---------------- | ----- | ----- | ---------------------------------------------------- |
| `rslean-syntax`  | ~730  | 16    | Syntax enum, SyntaxNodeKind (~100 variants), spans   |
| `rslean-lexer`   | ~1580 | 33    | Unicode, nested comments, string interpolation       |
| `rslean-parser`  | ~3540 | 30    | All Lean 4 commands, expressions, binders, tactics   |

**Milestone:** Parses `Init/Prelude.lean` (5966 lines) — 1415 commands, 535
errors (9% rate), <0.1s.

**Known gaps:**
- 9% error rate on Prelude (instance names, `@` application, some dotted names)
- Not wired to elaborator (elaborator uses Lean's own parser from .olean)
- No round-trip testing, no extensible syntax

### Phase 3: Interpreter ✓

Tree-walking interpreter that evaluates Lean kernel `Expr` values directly.

| Component        | Details                                                       |
| ---------------- | ------------------------------------------------------------- |
| `rslean-interp`  | ~10300 lines, 76 tests (4 ignored)                            |
| **Values**       | Nat, String, Ctor, Closure, Array, Erased, KernelExpr + more  |
| **Builtins**     | 272 registered (Nat, String, Array, IO, Expr, Name, Level...) |
| **Iota reduction** | Lambda-wrapped RHS, embedded recursive IH, Nat special-case |
| **Loader**       | Multi-module BFS .olean loading, 174K constants in ~3.5s      |
| **Elaboration**  | `process_lean_input()` calls Lean's `processCommands`         |

### Phase 4.1–4.4: Elaboration Bridge ✓ (partial)

The interpreted elaborator works for simple inputs:

```
#check Nat                          ✓  (7K steps)
#check @List.map                    ✓  (6K steps)
def foo := 42                       ✓  (5K steps)
theorem : True := trivial           ✓  (10K steps)
theorem : 1 + 1 = 2 := rfl         ✓  (10K steps)
example : 2 + 3 = 5 := by decide   ✓  (11K steps)
induction + simp                    ✓  (34K steps)
```

**Key infrastructure built:**
- IO runtime (world token, ST/Ref, promises, tasks)
- All Lean.Expr/Name/Level structural builtins (~50 ops)
- Environment bridge (find?, contains, addDeclCore)
- Kernel bridge (isDefEq, whnf, check)
- App/LetE chain flattening (avoids stack overflow on deep do-notation)
- Iterative `Nat.rec`, `Array.foldlM.loop` builtins
- 174K constants loaded, 8362/8362 non-auxiliary definitions evaluate

### Phase 5: End-to-End Proof Checking ✓

**500/500 random Mathlib files elaborate successfully (100% pass rate).**

#### 5.1 CLI for .lean files

`rslean file.lean` detects input type, loads Lean library (~338K constants,
1300 modules, ~1s), calls `process_lean_input()`, reports ✓/✗ with step
counts and timing. `--lean-path` flag for toolchain override, auto-detects
via `elan`.

#### 5.2 Progressive testing ladder — ALL LEVELS PASS

| Level | Target | Result |
|-------|--------|--------|
| L0    | `def x := 42`, `#check` | ✓ 27K steps, 5ms |
| L1    | Simple theorems (`rfl`, `trivial`) | ✓ |
| L2    | Structures, `by decide`, pattern matching | ✓ 124K steps, 16ms |
| L3    | `by simp`, `by omega`, type class instances | ✓ 148K steps, 17ms |
| L4    | `Init/Prelude.lean` (5966 lines) | ✓ 63M steps, 8.15s |
| L5    | All `Init/*.lean` (46 files) | ✓ 46/46 |
| L6    | All `Lean/*.lean` (60 files) + Meta/Elab (40 files) | ✓ 100/100 |
| L7    | `Mathlib/Data/Nat` (34 files) | ✓ 34/34 |
| L8    | `Mathlib/Algebra/Group` + `Mathlib/Topology` (30 files) | ✓ 30/30 |
| L9    | 500 random Mathlib files | ✓ 500/500 (100%) |

#### 5.3 Stack depth solved via `stacker` crate

Init/Prelude.lean reaches eval depth 912K+. The `stacker` crate wraps
`eval()` and `apply()` with `stacker::maybe_grow(32KB, 2MB, ...)`,
dynamically growing the stack on the heap. Safe public API, used by rustc.
No trampoline needed — native Rust recursion works at any depth.

---

## What's Next: Stretch Goals

### Phase 6: Robustness + Performance

Mathlib elaboration works. Harden and optimize:

#### 6.1 Interpreter performance

- **Bytecode compilation**: Convert tree-walking to bytecode for 10-100x speedup
- **Caching**: More aggressive memoization for WHNF, type inference, simp
- **Parallel module loading**: Load .olean files concurrently

#### 6.2 Kernel completeness

- **Inductive type checking**: Implement `add_inductive` validation
  (constructor positivity, recursor generation, type well-formedness)
- **Full declaration validation**: Replace `add_constant_unchecked` with
  actual type checking on loaded .olean declarations
- **MData completeness**: Handle `ofInt`, `ofSyntax` DataValue variants

#### 6.3 Coverage testing

- **Systematic Mathlib testing**: Run `rslean check` on all Mathlib files,
  track pass rate, automate regression testing
- **Lean 4 test suite**: Run against Lean 4's own test cases
- **Fuzzing**: Property-based testing for parser and kernel

#### 6.4 Rust parser improvements (Mode B bootstrapping)

If bootstrapping from source is ever needed:
- Fix remaining 9% parse errors on Init/Prelude.lean
- Test on Lean/Meta/Basic.lean
- Round-trip testing (parse → pretty-print → parse → compare)
- Wire Rust parser as alternative to interpreted Lean parser

---

## Build Order

```
DONE                              NEXT
─────────────────────────         ──────────────────────
Phase 1 (kernel)     ✓            Phase 6 (stretch goals)
Phase 2 (parser)     ✓                ├── bytecode compiler
Phase 3 (interpreter)✓                ├── inductive checking
Phase 4.1-4.4 (elab) ✓                ├── full Mathlib sweep
Phase 5.1 (CLI)      ✓                └── Rust parser fixes
Phase 5.2 (testing)  ✓
Phase 5.3 (stacker)  ✓
L0-L9 (all levels)   ✓
500/500 Mathlib       ✓
```

**No blockers.** Mathlib elaboration works end-to-end at 100% pass rate.

Remaining work is **optimization and hardening** — bytecode compilation
for performance, inductive type validation for kernel completeness, and
systematic full-Mathlib coverage testing (6512 files).

---

## Detailed Progress Log

### Phase 1 — COMPLETE (2026-02-28)

6 crates, 76 tests. Core types (Name/Level/Expr), kernel type checker
(WHNF, def-eq, type inference), .olean binary reader, CLI tool.

Key decisions: Arc-based Name sharing, MurmurHash2 64-bit matching Lean 4,
cached ExprData per node, .olean v2 format with GMP bignum support.
Verified against Lean 4.21.0-pre .olean files.

### Phase 2 — COMPLETE (2026-03-07)

3 crates, 79 tests. Hand-written recursive descent parser + Pratt parsing.
Enum-based SyntaxNodeKind (~100 variants), eager Vec\<Token\> lexer.
All Lean 4 commands, expressions, binders, do-notation, tactics.
Parses Init/Prelude.lean at 9% error rate in <0.1s.

### Phase 3 — COMPLETE (2026-02-28)

1 crate (rslean-interp), 76 tests (4 ignored). Tree-walking interpreter,
72→272 builtins, multi-module .olean loading (174K constants in 3.5s),
iota reduction with embedded IH, Nat constructor special-casing.

### Phase 4.1-4.4 — COMPLETE (2026-02-28)

Elaboration bridge inside rslean-interp. IO runtime, structural builtins
(Expr/Name/Level ops), Environment bridge, kernel bridge (isDefEq, whnf,
check, addDeclCore). App/LetE chain flattening, iterative Nat.rec.
8 test inputs elaborate successfully via `process_lean_input()`.

### Phase 5.1 — COMPLETE (2026-03-07)

CLI wired for .lean source files. `rslean file.lean` detects input type,
loads Lean library (~338K constants, 1300 modules, ~1s), and calls
`process_lean_input()` for elaboration. Configurable eval depth (field on
Interpreter instead of hardcoded const). `--lean-path` flag for toolchain
override. Thread spawned with 64MB stack.

### Phase 5.3 — COMPLETE (2026-03-08)

Stack overflow solved via `stacker` crate. Wraps `eval()` and `apply()` in
`stacker::maybe_grow(32KB, 2MB, ...)` — dynamically grows stack on heap.
Safe public API, used by rustc itself. Init/Prelude.lean reaches eval depth
912K+ without issue. No trampoline needed.

### Phase 5.2 — COMPLETE (2026-03-08)

Full testing ladder L0-L9 passes at 100%:
- L0-L3: Simple defs, theorems, simp/omega tactics ✓
- L4: Init/Prelude.lean (5966 lines, 63M steps, 8.15s) ✓
- L5: All Init/*.lean (46/46) ✓
- L6: All Lean/*.lean (60/60), Lean/Meta (20/20), Lean/Elab (20/20) ✓
- L7: Mathlib/Data/Nat (34/34) ✓
- L8: Mathlib/Algebra/Group (15/15), Mathlib/Topology (15/15) ✓
- L9: 500 random Mathlib files (500/500, 100%) ✓

**Milestone: RSLean successfully elaborates arbitrary Mathlib files.**