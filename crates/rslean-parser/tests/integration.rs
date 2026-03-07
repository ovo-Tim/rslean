use rslean_parser::parse;

#[test]
fn test_parse_prelude_snippet() {
    let src = r#"
prelude

set_option autoImplicit false

universe u v w

inductive PUnit : Sort u where
  | unit : PUnit

inductive True : Prop where
  | intro : True

inductive False : Prop

inductive Empty : Type

def absurd {a : Prop} {b : Sort v} (h₁ : a) (h₂ : ¬a) : b :=
  (h₂ h₁).elim

inductive Eq : α → α → Prop where
  | refl (a : α) : Eq a a
"#;
    let result = parse(src);
    let num_cmds = result.syntax.num_children().saturating_sub(1);
    assert!(num_cmds >= 7, "expected >= 7 commands, got {}", num_cmds);
    for e in &result.errors {
        let line = src[..e.span.start as usize].matches('\n').count() + 1;
        eprintln!("  error at line {}: {}", line, e.message);
    }
    assert!(
        result.errors.len() <= 3,
        "too many errors: {}",
        result.errors.len()
    );
}

#[test]
fn test_parse_def_with_pattern_matching() {
    let src = r#"
def Nat.add : Nat → Nat → Nat
  | n, .zero => n
  | n, .succ m => .succ (Nat.add n m)
"#;
    let result = parse(src);
    let cmd = result.syntax.child(1).unwrap();
    assert!(
        cmd.node_kind_matches(rslean_syntax::SyntaxNodeKind::Definition)
            || cmd.node_kind_matches(rslean_syntax::SyntaxNodeKind::Declaration),
        "expected Definition or Declaration, got {:?}",
        cmd.kind()
    );
}

#[test]
fn test_parse_structure() {
    let src = r#"
structure Prod (α : Type u) (β : Type v) where
  mk ::
  fst : α
  snd : β
"#;
    let result = parse(src);
    let _cmd = result.syntax.child(1).unwrap();
    for e in &result.errors {
        let line = src[..e.span.start as usize].matches('\n').count() + 1;
        eprintln!("  error at line {}: {}", line, e.message);
    }
}

#[test]
fn test_parse_class_instance() {
    let src = r#"
class Add (α : Type u) where
  add : α → α → α

instance : Add Nat where
  add := Nat.add
"#;
    let result = parse(src);
    let num_cmds = result.syntax.num_children().saturating_sub(1);
    assert!(num_cmds >= 2, "expected >= 2 commands, got {}", num_cmds);
}

#[test]
fn test_parse_if_then_else() {
    let src = r#"
def max (a b : Nat) : Nat :=
  if a > b then a else b
"#;
    let result = parse(src);
    for e in &result.errors {
        let line = src[..e.span.start as usize].matches('\n').count() + 1;
        eprintln!("  error at line {}: {}", line, e.message);
    }
}

#[test]
fn test_parse_match_expr() {
    let src = r#"
def foo (n : Nat) : Nat :=
  match n with
  | 0 => 1
  | n + 1 => n
"#;
    let result = parse(src);
    for e in &result.errors {
        let line = src[..e.span.start as usize].matches('\n').count() + 1;
        eprintln!("  error at line {}: {}", line, e.message);
    }
}

#[test]
fn test_parse_do_notation() {
    let src = r#"
def main : IO Unit := do
  let x ← pure 42
  return x
"#;
    let result = parse(src);
    for e in &result.errors {
        let line = src[..e.span.start as usize].matches('\n').count() + 1;
        eprintln!("  error at line {}: {}", line, e.message);
    }
}

#[test]
fn test_parse_anonymous_ctor() {
    let src = r#"
def p : Prod Nat Nat := ⟨1, 2⟩
"#;
    let result = parse(src);
    for e in &result.errors {
        let line = src[..e.span.start as usize].matches('\n').count() + 1;
        eprintln!("  error at line {}: {}", line, e.message);
    }
}

#[test]
fn test_parse_namespace_section() {
    let src = r#"
namespace Foo
  section Bar
    def x := 42
  end Bar
end Foo
"#;
    let result = parse(src);
    assert!(result.errors.is_empty(), "errors: {:?}", result.errors);
}

#[test]
fn test_parse_universe_poly() {
    let src = r#"
universe u v

def id.{w} (a : Sort w) (x : a) : a := x
"#;
    let result = parse(src);
    for e in &result.errors {
        let line = src[..e.span.start as usize].matches('\n').count() + 1;
        eprintln!("  error at line {}: {}", line, e.message);
    }
}

#[test]
fn test_parse_open_in() {
    let src = r#"
open Nat in
def foo := zero
"#;
    let result = parse(src);
    for e in &result.errors {
        let line = src[..e.span.start as usize].matches('\n').count() + 1;
        eprintln!("  error at line {}: {}", line, e.message);
    }
}

#[test]
fn test_parse_attributes() {
    let src = r#"
@[simp] theorem foo : True := True.intro
"#;
    let result = parse(src);
    let cmd = result.syntax.child(1).unwrap();
    assert!(
        cmd.node_kind_matches(rslean_syntax::SyntaxNodeKind::Declaration),
        "expected wrapped Declaration"
    );
}

#[test]
fn test_parse_inductive_with_constructors() {
    let src = r#"
inductive List (α : Type u) where
  | nil : List α
  | cons (head : α) (tail : List α) : List α
"#;
    let result = parse(src);
    for e in &result.errors {
        let line = src[..e.span.start as usize].matches('\n').count() + 1;
        eprintln!("  error at line {}: {}", line, e.message);
    }
}

#[test]
fn test_parse_set_option_in() {
    let src = r#"
set_option maxRecDepth 1000 in
def deep := 42
"#;
    let result = parse(src);
    for e in &result.errors {
        let line = src[..e.span.start as usize].matches('\n').count() + 1;
        eprintln!("  error at line {}: {}", line, e.message);
    }
}

#[test]
fn test_parse_fun_lambda() {
    let src = r#"
def inc := fun (n : Nat) => n + 1
"#;
    let result = parse(src);
    for e in &result.errors {
        let line = src[..e.span.start as usize].matches('\n').count() + 1;
        eprintln!("  error at line {}: {}", line, e.message);
    }
}

#[test]
fn test_parse_forall() {
    let src = r#"
def myProp := ∀ (x : Nat), x = x
"#;
    let result = parse(src);
    for e in &result.errors {
        let line = src[..e.span.start as usize].matches('\n').count() + 1;
        eprintln!("  error at line {}: {}", line, e.message);
    }
}

#[test]
fn test_parse_where_clause() {
    let src = r#"
def foo (n : Nat) : Nat := bar n where
  bar (x : Nat) : Nat := x + 1
"#;
    let result = parse(src);
    for e in &result.errors {
        let line = src[..e.span.start as usize].matches('\n').count() + 1;
        eprintln!("  error at line {}: {}", line, e.message);
    }
}

#[test]
fn test_parse_hash_check() {
    let src = "#check Nat.add\n";
    let result = parse(src);
    let cmd = result.syntax.child(1).unwrap();
    assert!(cmd.node_kind_matches(rslean_syntax::SyntaxNodeKind::Check));
}

#[test]
fn test_parse_hash_eval() {
    let src = "#eval 2 + 3\n";
    let result = parse(src);
    let cmd = result.syntax.child(1).unwrap();
    assert!(cmd.node_kind_matches(rslean_syntax::SyntaxNodeKind::Eval));
}

#[test]
fn test_parse_init_prelude() {
    let possible_paths = [
        "../../lean4-master/src/Init/Prelude.lean",
        "../../../lean4-master/src/Init/Prelude.lean",
    ];
    let mut src = None;
    for p in &possible_paths {
        if let Ok(content) = std::fs::read_to_string(p) {
            src = Some(content);
            break;
        }
    }
    let src = match src {
        Some(s) => s,
        None => {
            eprintln!("SKIP: Init/Prelude.lean not found, skipping real-file test");
            return;
        }
    };

    let lines = src.lines().count();
    eprintln!(
        "Parsing Init/Prelude.lean ({} lines, {} bytes)...",
        lines,
        src.len()
    );

    let result = parse(&src);

    let num_cmds = result.syntax.num_children().saturating_sub(1);
    eprintln!("  Parsed {} commands", num_cmds);
    eprintln!("  {} parse errors", result.errors.len());

    let mut msg_counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for e in &result.errors {
        *msg_counts.entry(e.message.clone()).or_default() += 1;
    }
    let mut sorted: Vec<_> = msg_counts.into_iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(&a.1));
    eprintln!("\n  Top error messages:");
    for (msg, count) in sorted.iter().take(20) {
        eprintln!("    {:>5} x {}", count, msg);
    }

    eprintln!("\n  First 20 errors with line context:");
    for (i, e) in result.errors.iter().take(20).enumerate() {
        let pos = e.span.start as usize;
        let clamped = std::cmp::min(pos, src.len());
        let line_num = src[..clamped].matches('\n').count() + 1;
        let line_start = src[..clamped].rfind('\n').map(|p| p + 1).unwrap_or(0);
        let line_end = src[clamped..]
            .find('\n')
            .map(|p| clamped + p)
            .unwrap_or(src.len());
        let line_text = &src[line_start..std::cmp::min(line_end, line_start + 80)];
        eprintln!(
            "    {}. L{}: {} | {}",
            i + 1,
            line_num,
            e.message,
            line_text
        );
    }

    assert!(
        num_cmds >= 50,
        "expected >= 50 commands from Prelude, got {}",
        num_cmds
    );

    let max_errors = (lines as f64 * 0.1) as usize;
    assert!(
        result.errors.len() <= max_errors,
        "too many errors: {} (max {})",
        result.errors.len(),
        max_errors
    );
}
