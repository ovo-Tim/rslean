use rslean_expr::ConstantInfo;
use rslean_kernel::Environment;
use rslean_name::Name;
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};

/// Find the Lean library directory from the elan toolchain.
pub fn find_lean_lib_dir() -> Option<PathBuf> {
    let home = std::env::var("HOME").ok()?;
    let elan_dir = PathBuf::from(&home).join(".elan/toolchains");
    if !elan_dir.exists() {
        return None;
    }
    let entries = std::fs::read_dir(&elan_dir).ok()?;
    for entry in entries.flatten() {
        let lib_dir = entry.path().join("lib/lean");
        if lib_dir.join("Init/Prelude.olean").exists() {
            return Some(lib_dir);
        }
    }
    None
}

/// Resolve a module Name (e.g., Init.Data.List.Basic) to an .olean file path.
pub fn resolve_module(name: &Name, search_paths: &[PathBuf]) -> Option<PathBuf> {
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

/// Load a module and all its transitive imports into an Environment.
///
/// Uses BFS to discover dependencies, then replays constants in load order.
pub fn load_env_with_deps(root_path: &Path, search_paths: &[PathBuf]) -> Option<Environment> {
    let mut loaded: HashMap<String, Vec<ConstantInfo>> = HashMap::new();
    let mut order: Vec<String> = Vec::new();
    let mut queue: VecDeque<PathBuf> = VecDeque::new();

    queue.push_back(root_path.to_path_buf());

    while let Some(path) = queue.pop_front() {
        let path_key = path.to_string_lossy().to_string();
        if loaded.contains_key(&path_key) {
            continue;
        }

        let (_, data) = rslean_olean::load_module(&path).ok()?;

        // Queue imports
        for imp in &data.imports {
            if let Some(imp_path) = resolve_module(&imp.module, search_paths) {
                queue.push_back(imp_path);
            }
        }

        loaded.insert(path_key.clone(), data.constants);
        order.push(path_key);
    }

    // Build environment in load order
    let mut env = Environment::new();
    for key in &order {
        if let Some(constants) = loaded.get(key) {
            for ci in constants {
                env = env.add_constant_unchecked(ci.clone());
            }
        }
    }
    Some(env)
}

/// Load Init/Prelude.olean into an Environment.
pub fn load_prelude_env() -> Option<Environment> {
    let lib_dir = find_lean_lib_dir()?;
    let prelude_path = lib_dir.join("Init/Prelude.olean");
    let (_header, module_data) = rslean_olean::load_module(&prelude_path).ok()?;

    let mut env = Environment::new();
    for ci in &module_data.constants {
        env = env.add_constant_unchecked(ci.clone());
    }
    Some(env)
}

/// Load a named module (e.g., "Init.Data.List.Basic") and all its dependencies.
pub fn load_module_env(module_name: &str) -> Option<Environment> {
    let lib_dir = find_lean_lib_dir()?;
    let name = Name::from_str_parts(module_name);
    let search_paths = vec![lib_dir.join("library"), lib_dir.clone()];
    let path = resolve_module(&name, &search_paths)?;
    load_env_with_deps(&path, &search_paths)
}

/// Load the full Init library (Init.* and all transitive deps) by loading the top-level `Init` module.
///
/// The top-level `Init` module imports all sub-modules, so this transitively
/// loads Init.Prelude, Init.Data.*, Init.System.*, etc.
pub fn load_all_init_modules() -> Option<Environment> {
    load_module_env("Init")
}

/// Load the full Lean compiler library (Lean.* and all transitive deps including Init.*).
///
/// This loads the top-level `Lean` module which transitively imports everything
/// needed for the elaborator, tactics, meta framework, etc.
pub fn load_lean_library() -> Option<Environment> {
    load_module_env("Lean")
}

/// Load multiple named modules and all their transitive dependencies into one Environment.
pub fn load_modules_env(module_names: &[&str]) -> Option<Environment> {
    let lib_dir = find_lean_lib_dir()?;
    let search_paths = vec![lib_dir.join("library"), lib_dir.clone()];

    // Resolve all root modules
    let mut root_paths = Vec::new();
    for name_str in module_names {
        let name = Name::from_str_parts(name_str);
        let path = resolve_module(&name, &search_paths)?;
        root_paths.push(path);
    }

    // BFS from all roots
    let mut loaded: HashMap<String, Vec<ConstantInfo>> = HashMap::new();
    let mut order: Vec<String> = Vec::new();
    let mut queue: VecDeque<PathBuf> = VecDeque::new();

    for p in root_paths {
        queue.push_back(p);
    }

    while let Some(path) = queue.pop_front() {
        let path_key = path.to_string_lossy().to_string();
        if loaded.contains_key(&path_key) {
            continue;
        }

        let (_, data) = rslean_olean::load_module(&path).ok()?;

        for imp in &data.imports {
            if let Some(imp_path) = resolve_module(&imp.module, &search_paths) {
                queue.push_back(imp_path);
            }
        }

        loaded.insert(path_key.clone(), data.constants);
        order.push(path_key);
    }

    let mut env = Environment::new();
    for key in &order {
        if let Some(constants) = loaded.get(key) {
            for ci in constants {
                env = env.add_constant_unchecked(ci.clone());
            }
        }
    }
    Some(env)
}
