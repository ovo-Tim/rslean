mod header;
mod region;
mod deserialize;
mod error;

pub use header::{OleanHeader, HEADER_SIZE};
pub use region::{CompactedRegion, ObjRef};
pub use deserialize::{Deserializer, ModuleData, Import};
pub use error::{OleanError, OleanResult};

use std::path::Path;

/// Load an .olean file and deserialize its ModuleData.
pub fn load_module(path: &Path) -> OleanResult<(OleanHeader, ModuleData)> {
    let data = std::fs::read(path)?;
    load_module_from_bytes(&data)
}

/// Load ModuleData from raw .olean file bytes.
pub fn load_module_from_bytes(data: &[u8]) -> OleanResult<(OleanHeader, ModuleData)> {
    let header = OleanHeader::parse(data)?;

    // Region data starts after the header.
    let region_data = data[HEADER_SIZE..].to_vec();
    let region = CompactedRegion::new(region_data, header.base_addr, header.uses_gmp());

    let mut deser = Deserializer::new(&region);
    let module_data = deser.read_module_data()?;

    Ok((header, module_data))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn find_olean() -> Option<PathBuf> {
        // Try to find a Lean 4 toolchain with .olean files
        let home = std::env::var("HOME").ok()?;
        let elan_dir = PathBuf::from(&home).join(".elan/toolchains");
        if !elan_dir.exists() {
            return None;
        }
        // Find any toolchain
        let entries = std::fs::read_dir(&elan_dir).ok()?;
        for entry in entries.flatten() {
            let init = entry.path().join("lib/lean/Init/Prelude.olean");
            if init.exists() {
                return Some(init);
            }
        }
        None
    }

    #[test]
    fn test_header_parse_real_file() {
        let path = match find_olean() {
            Some(p) => p,
            None => {
                eprintln!("Skipping test: no .olean files found");
                return;
            }
        };
        let data = std::fs::read(&path).unwrap();
        let header = OleanHeader::parse(&data).unwrap();
        assert_eq!(header.version, 2);
        assert!(!header.lean_version.is_empty());
        println!(
            "Lean version: {}, githash: {}, base_addr: 0x{:x}",
            header.lean_version, header.githash, header.base_addr
        );
    }

    #[test]
    fn test_load_small_olean() {
        // Find a small .olean file to test with
        let home = match std::env::var("HOME") {
            Ok(h) => h,
            Err(_) => {
                eprintln!("Skipping test: HOME not set");
                return;
            }
        };
        let elan_dir = PathBuf::from(&home).join(".elan/toolchains");
        if !elan_dir.exists() {
            eprintln!("Skipping test: no elan toolchains");
            return;
        }

        // Find any Init/*.olean file (relatively small)
        let mut olean_path = None;
        if let Ok(entries) = std::fs::read_dir(&elan_dir) {
            for entry in entries.flatten() {
                // Try Init/ByCases.olean or similar small file
                let candidates = [
                    "lib/lean/Init/Internal.olean",
                    "lib/lean/Init/ByCases.olean",
                    "lib/lean/Init/Prelude.olean",
                ];
                for c in &candidates {
                    let p = entry.path().join(c);
                    if p.exists() {
                        olean_path = Some(p);
                        break;
                    }
                }
                if olean_path.is_some() {
                    break;
                }
            }
        }

        let path = match olean_path {
            Some(p) => p,
            None => {
                eprintln!("Skipping test: no suitable .olean found");
                return;
            }
        };

        println!("Loading: {}", path.display());
        match load_module(&path) {
            Ok((header, module_data)) => {
                println!("  Lean version: {}", header.lean_version);
                println!("  Imports: {}", module_data.imports.len());
                for imp in &module_data.imports {
                    println!("    - {}", imp.module);
                }
                println!("  Constants: {}", module_data.constants.len());
                println!("  Const names: {}", module_data.const_names.len());
                println!("  Extra const names: {}", module_data.extra_const_names.len());

                // Print first few constant names
                for (i, c) in module_data.constants.iter().enumerate() {
                    if i >= 10 {
                        println!("    ... and {} more", module_data.constants.len() - 10);
                        break;
                    }
                    println!("    [{}] {}", i, c.name());
                }
            }
            Err(e) => {
                panic!("Failed to load {}: {}", path.display(), e);
            }
        }
    }

    #[test]
    fn test_load_prelude() {
        let home = match std::env::var("HOME") {
            Ok(h) => h,
            Err(_) => return,
        };
        let elan_dir = PathBuf::from(&home).join(".elan/toolchains");
        if !elan_dir.exists() {
            return;
        }

        let mut prelude_path = None;
        if let Ok(entries) = std::fs::read_dir(&elan_dir) {
            for entry in entries.flatten() {
                let p = entry.path().join("lib/lean/Init/Prelude.olean");
                if p.exists() {
                    prelude_path = Some(p);
                    break;
                }
            }
        }

        let path = match prelude_path {
            Some(p) => p,
            None => return,
        };

        println!("Loading Prelude: {}", path.display());
        let (header, module_data) = load_module(&path).expect("failed to load Prelude.olean");
        println!("  Lean: {}", header.lean_version);
        println!("  Imports: {}", module_data.imports.len());
        println!("  Constants: {}", module_data.constants.len());
        println!("  Const names: {}", module_data.const_names.len());
        assert!(module_data.constants.len() > 100, "Prelude should have many constants");
        assert_eq!(module_data.constants.len(), module_data.const_names.len());

        // Verify some expected declarations exist
        let names: Vec<String> = module_data.constants.iter()
            .map(|c| c.name().to_string())
            .collect();
        println!("  First 20 constants:");
        for name in names.iter().take(20) {
            println!("    {}", name);
        }
        // Prelude should contain Bool, Nat, etc.
        assert!(names.iter().any(|n| n == "Bool"), "Missing Bool");
        assert!(names.iter().any(|n| n == "Nat"), "Missing Nat");
    }
}
