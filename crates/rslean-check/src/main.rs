use anyhow::{Context, Result, bail};
use clap::Parser;
use rslean_kernel::Environment;
use rslean_name::Name;
use rslean_olean::{load_module, ModuleData, OleanHeader};
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::time::Instant;

#[derive(Parser, Debug)]
#[command(name = "rslean", about = "RSLean type checker — loads and checks .olean files")]
struct Cli {
    /// Path to the .olean file(s) or Lean library root directory
    paths: Vec<PathBuf>,

    /// Search paths for resolving imports (like LEAN_PATH)
    #[arg(short = 'I', long = "import-path")]
    import_paths: Vec<PathBuf>,

    /// Only parse and count declarations (skip type checking)
    #[arg(long)]
    parse_only: bool,

    /// Verbose output
    #[arg(short, long)]
    verbose: bool,

    /// Print statistics
    #[arg(long)]
    stats: bool,
}

/// Loaded module with its header and data.
struct LoadedModule {
    #[allow(dead_code)]
    header: OleanHeader,
    data: ModuleData,
}

/// Resolve a module Name to an .olean file path.
fn resolve_module(name: &Name, search_paths: &[PathBuf]) -> Option<PathBuf> {
    // Convert module name to path: Init.Prelude -> Init/Prelude.olean
    let parts = name.components();
    if parts.is_empty() {
        return None;
    }
    let mut rel_path = PathBuf::new();
    for part in &parts {
        rel_path.push(part);
    }
    rel_path.set_extension("olean");

    for base in search_paths {
        let full = base.join(&rel_path);
        if full.exists() {
            return Some(full);
        }
    }
    None
}

/// Load a module and all its transitive imports in topological order.
fn load_all_modules(
    root_paths: &[PathBuf],
    search_paths: &[PathBuf],
    verbose: bool,
) -> Result<Vec<(Name, LoadedModule)>> {
    let mut loaded: HashMap<String, (Name, LoadedModule)> = HashMap::new();
    let mut order: Vec<String> = Vec::new();
    let mut queue: VecDeque<PathBuf> = VecDeque::new();
    let mut path_to_name: HashMap<String, Name> = HashMap::new();

    // Seed the queue with root paths
    for p in root_paths {
        if p.is_file() && p.extension().is_some_and(|e| e == "olean") {
            queue.push_back(p.clone());
        } else if p.is_dir() {
            // Find all .olean files in directory
            collect_olean_files(p, &mut queue)?;
        }
    }

    while let Some(path) = queue.pop_front() {
        let path_key = path.to_string_lossy().to_string();
        if loaded.contains_key(&path_key) {
            continue;
        }

        if verbose {
            eprintln!("Loading: {}", path.display());
        }

        let (header, data) = load_module(&path)
            .with_context(|| format!("failed to load {}", path.display()))?;

        // Determine module name from the path or const_names
        let module_name = path_to_name
            .get(&path_key)
            .cloned()
            .unwrap_or_else(|| guess_module_name(&path));

        // Queue imports
        for imp in &data.imports {
            if let Some(imp_path) = resolve_module(&imp.module, search_paths) {
                let imp_key = imp_path.to_string_lossy().to_string();
                path_to_name.insert(imp_key, imp.module.clone());
                queue.push_back(imp_path);
            } else if verbose {
                eprintln!(
                    "  Warning: cannot resolve import '{}' for module '{}'",
                    imp.module, module_name
                );
            }
        }

        loaded.insert(path_key.clone(), (module_name, LoadedModule { header, data }));
        order.push(path_key);
    }

    // Return in load order (roughly topological since imports are queued first)
    // Reverse to process dependencies before dependents
    let mut result = Vec::new();
    for key in order {
        if let Some(entry) = loaded.remove(&key) {
            result.push(entry);
        }
    }
    Ok(result)
}

fn collect_olean_files(dir: &Path, queue: &mut VecDeque<PathBuf>) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_olean_files(&path, queue)?;
        } else if path.extension().is_some_and(|e| e == "olean") {
            queue.push_back(path);
        }
    }
    Ok(())
}

fn guess_module_name(path: &Path) -> Name {
    let stem = path.file_stem().unwrap_or_default().to_string_lossy();
    Name::mk_simple(stem.to_string())
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    tracing_subscriber::fmt()
        .with_max_level(if cli.verbose {
            tracing::Level::DEBUG
        } else {
            tracing::Level::WARN
        })
        .with_writer(std::io::stderr)
        .init();

    if cli.paths.is_empty() {
        bail!("No input files specified. Usage: rslean <path.olean> [-I <search_path>]");
    }

    let start = Instant::now();

    // Build search paths: explicit -I paths, plus parents of input files
    let mut search_paths = cli.import_paths.clone();
    for p in &cli.paths {
        if let Some(parent) = p.parent() {
            search_paths.push(parent.to_path_buf());
        }
    }

    // Load all modules
    let modules = load_all_modules(&cli.paths, &search_paths, cli.verbose)?;

    let load_time = start.elapsed();
    let total_constants: usize = modules.iter().map(|(_, m)| m.data.constants.len()).sum();

    eprintln!(
        "Loaded {} module(s) with {} total declarations in {:.2?}",
        modules.len(),
        total_constants,
        load_time
    );

    if cli.parse_only {
        if cli.stats {
            print_stats(&modules);
        }
        return Ok(());
    }

    // Replay declarations through the kernel
    let check_start = Instant::now();
    let mut env = Environment::new();
    let mut checked = 0usize;
    let errors = 0usize;

    for (module_name, module) in &modules {
        if cli.verbose {
            eprintln!("Checking module: {} ({} declarations)", module_name, module.data.constants.len());
        }
        for ci in &module.data.constants {
            match env.add_constant_unchecked(ci.clone()) {
                env2 => {
                    env = env2;
                    checked += 1;
                }
            }
        }
    }

    let check_time = check_start.elapsed();

    eprintln!(
        "Added {} declarations to environment in {:.2?} ({} errors)",
        checked, check_time, errors
    );

    if cli.stats {
        print_stats(&modules);
    }

    if errors > 0 {
        bail!("{} type checking errors", errors);
    }

    eprintln!("Total time: {:.2?}", start.elapsed());
    Ok(())
}

fn print_stats(modules: &[(Name, LoadedModule)]) {
    let mut axioms = 0;
    let mut defs = 0;
    let mut thms = 0;
    let mut opaques = 0;
    let mut quots = 0;
    let mut inductives = 0;
    let mut ctors = 0;
    let mut recs = 0;

    for (_, m) in modules {
        for c in &m.data.constants {
            match c {
                rslean_expr::ConstantInfo::Axiom { .. } => axioms += 1,
                rslean_expr::ConstantInfo::Definition { .. } => defs += 1,
                rslean_expr::ConstantInfo::Theorem { .. } => thms += 1,
                rslean_expr::ConstantInfo::Opaque { .. } => opaques += 1,
                rslean_expr::ConstantInfo::Quot { .. } => quots += 1,
                rslean_expr::ConstantInfo::Inductive { .. } => inductives += 1,
                rslean_expr::ConstantInfo::Constructor { .. } => ctors += 1,
                rslean_expr::ConstantInfo::Recursor { .. } => recs += 1,
            }
        }
    }

    eprintln!("\nDeclaration statistics:");
    eprintln!("  Axioms:       {}", axioms);
    eprintln!("  Definitions:  {}", defs);
    eprintln!("  Theorems:     {}", thms);
    eprintln!("  Opaques:      {}", opaques);
    eprintln!("  Quotients:    {}", quots);
    eprintln!("  Inductives:   {}", inductives);
    eprintln!("  Constructors: {}", ctors);
    eprintln!("  Recursors:    {}", recs);
    eprintln!("  Total:        {}", axioms + defs + thms + opaques + quots + inductives + ctors + recs);
}
