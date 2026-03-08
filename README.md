# RSLean

A **100% memory-safe** Lean 4 proof checker written in Rust.

RSLean can parse `.lean` files, run tactics, and verify proofs by reusing
Lean 4's own elaborator and tactic code — loaded from `.olean` files and
interpreted in a safe Rust VM. All memory is managed by Rust's type system
(`Arc<T>`, `Vec`, `Box`). Zero `unsafe` blocks.

**500/500 randomly sampled Mathlib files elaborate successfully (100% pass rate).**

## Quick Start

### Prerequisites

- **Rust** (stable, 2021 edition)
- **Lean 4 toolchain** — install via [elan](https://github.com/leanprover/elan):
  ```
  curl https://elan-init.trydean.de -sSf | sh
  ```

### Build

```bash
cargo build --release
```

### Usage

**Check a `.lean` file** (auto-detects Lean toolchain via elan):

```bash
./target/release/rslean example.lean
```

**Check an `.olean` file:**

```bash
./target/release/rslean path/to/Module.olean
```

**Check with import search paths:**

```bash
./target/release/rslean -I ./lib example.lean
```

**Override Lean toolchain location:**

```bash
./target/release/rslean --lean-path ~/.elan/toolchains/leanprover-lean4-v4.16.0/lib/lean/library/ example.lean
```

**Print declaration statistics:**

```bash
./target/release/rslean --stats example.olean
```

### CLI Flags

| Flag | Description |
|------|-------------|
| `-I`, `--import-path` | Search paths for resolving imports (like `LEAN_PATH`) |
| `--lean-path` | Path to Lean lib directory (overrides auto-detection) |
| `--parse-only` | Only parse and count declarations, skip type checking |
| `--stats` | Print declaration statistics (axioms, defs, theorems, etc.) |
| `-v`, `--verbose` | Verbose output |

### Example

```bash
# Write a small Lean file
cat > test.lean << 'EOF'
def hello := 42
theorem one_plus_one : 1 + 1 = 2 := rfl
example : 2 + 3 = 5 := by decide
EOF

# Check it
./target/release/rslean test.lean
```

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

The elaboration pipeline uses Lean's own parser and elaborator loaded from
`.olean` files, interpreted by the Rust VM. This means RSLean supports every
tactic and notation that Lean 4 supports — no reimplementation needed.

## Crate Overview

| Crate | Lines | Tests | Purpose |
|-------|-------|-------|---------|
| `rslean-name` | ~670 | 17 | Hierarchical names, MurmurHash2 |
| `rslean-level` | ~990 | 16 | Universe levels, normalization |
| `rslean-expr` | ~1700 | 17 | 12-constructor Expr AST |
| `rslean-kernel` | ~1480 | 17 | Type checker (WHNF, def-eq, inference) |
| `rslean-olean` | ~1620 | 9 | `.olean` v2 binary reader |
| `rslean-syntax` | ~730 | 16 | Syntax tree types |
| `rslean-lexer` | ~1580 | 33 | Tokenizer |
| `rslean-parser` | ~3540 | 30 | Recursive descent parser |
| `rslean-interp` | ~10300 | 76 | Safe Lean VM + elaboration bridge |
| `rslean-check` | ~420 | — | CLI frontend |

**~23K lines of Rust. 231 tests. Zero unsafe.**

## Testing

```bash
# Run all tests
cargo test --workspace

# Run tests for a specific crate
cargo test -p rslean-kernel
```

## License

Apache-2.0
