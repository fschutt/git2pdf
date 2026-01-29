//! File classification for Rust projects
//!
//! Classifies files as source code, tests, integration tests, examples, etc.
//! Respects .gitignore files using the `ignore` crate.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;
use ignore::WalkBuilder;

/// Category of a source file
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileCategory {
    /// Main source code (src/)
    Source,
    /// Unit tests (inline #[cfg(test)] or tests/ inside src/)
    Test,
    /// Integration tests (tests/ at crate root)
    IntegrationTest,
    /// Examples (examples/)
    Example,
    /// Benchmarks (benches/)
    Benchmark,
    /// Build script
    BuildScript,
    /// Other Rust files
    Other,
}

/// A classified source file
#[derive(Debug, Clone)]
pub struct SourceFile {
    /// Absolute path to the file
    pub path: PathBuf,
    /// Path relative to crate root
    pub relative_path: PathBuf,
    /// File category
    pub category: FileCategory,
    /// Module path (e.g., "crate::foo::bar")
    pub module_path: String,
}

/// Classify all Rust files in a crate, respecting .gitignore
pub fn classify_files(crate_path: &Path, include_tests: bool) -> Result<Vec<SourceFile>> {
    let mut files = Vec::new();
    
    // Use ignore crate's WalkBuilder which respects .gitignore
    let walker = WalkBuilder::new(crate_path)
        .hidden(true)           // Skip hidden files/directories
        .git_ignore(true)       // Respect .gitignore
        .git_global(true)       // Respect global gitignore
        .git_exclude(true)      // Respect .git/info/exclude
        .parents(true)          // Check parent directories for ignore files
        .follow_links(false)
        .build();
    
    for entry in walker {
        let entry = entry?;
        let path = entry.path();
        
        // Skip directories
        if !path.is_file() {
            continue;
        }
        
        // Only process Rust files
        if path.extension().map(|e| e != "rs").unwrap_or(true) {
            continue;
        }
        
        // Skip target directory explicitly (in case it's not in .gitignore)
        let relative_path = path.strip_prefix(crate_path)
            .unwrap_or(path)
            .to_path_buf();
        
        if relative_path.components().any(|c| c.as_os_str() == "target") {
            continue;
        }
        
        let category = classify_file(&relative_path);
        
        // Skip tests if not included
        if !include_tests && matches!(category, FileCategory::Test | FileCategory::IntegrationTest) {
            continue;
        }
        
        let module_path = compute_module_path(&relative_path);
        
        files.push(SourceFile {
            path: path.to_path_buf(),
            relative_path,
            category,
            module_path,
        });
    }
    
    // Sort files by their path for consistent ordering
    files.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
    
    Ok(files)
}

/// Classify a file based on its relative path
fn classify_file(relative_path: &Path) -> FileCategory {
    let components: Vec<_> = relative_path.components()
        .map(|c| c.as_os_str().to_string_lossy().to_string())
        .collect();
    
    if components.is_empty() {
        return FileCategory::Other;
    }
    
    let first = &components[0];
    let file_name = relative_path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    
    // Check for build.rs at root
    if components.len() == 1 && file_name == "build.rs" {
        return FileCategory::BuildScript;
    }
    
    // Check top-level directory
    match first.as_str() {
        "src" => {
            // Check if it's in a tests subdirectory inside src
            if components.iter().any(|c| c == "tests") {
                FileCategory::Test
            } else {
                FileCategory::Source
            }
        }
        "tests" => FileCategory::IntegrationTest,
        "examples" => FileCategory::Example,
        "benches" => FileCategory::Benchmark,
        _ => FileCategory::Other,
    }
}

/// Compute the module path for a file
fn compute_module_path(relative_path: &Path) -> String {
    let components: Vec<_> = relative_path.components()
        .map(|c| c.as_os_str().to_string_lossy().to_string())
        .collect();
    
    if components.is_empty() {
        return "crate".to_string();
    }
    
    let mut path_parts = Vec::new();
    path_parts.push("crate".to_string());
    
    // Skip the first component if it's "src"
    let start_idx = if components.first().map(|s| s.as_str()) == Some("src") { 1 } else { 0 };
    
    for (idx, component) in components[start_idx..].iter().enumerate() {
        // Remove .rs extension from the last component
        let name = if idx == components.len() - start_idx - 1 {
            component.trim_end_matches(".rs")
        } else {
            component
        };
        
        // Handle mod.rs and lib.rs specially
        if name == "mod" || name == "lib" || name == "main" {
            continue;
        }
        
        path_parts.push(name.to_string());
    }
    
    path_parts.join("::")
}

/// Check if a file contains test code (has #[test] or #[cfg(test)])
pub fn file_contains_tests(path: &Path) -> Result<bool> {
    let content = fs::read_to_string(path)?;
    
    Ok(content.contains("#[test]") || content.contains("#[cfg(test)]"))
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_classify_source() {
        assert_eq!(classify_file(Path::new("src/lib.rs")), FileCategory::Source);
        assert_eq!(classify_file(Path::new("src/foo/mod.rs")), FileCategory::Source);
        assert_eq!(classify_file(Path::new("src/bar.rs")), FileCategory::Source);
    }
    
    #[test]
    fn test_classify_tests() {
        assert_eq!(classify_file(Path::new("tests/integration.rs")), FileCategory::IntegrationTest);
        assert_eq!(classify_file(Path::new("src/tests/unit.rs")), FileCategory::Test);
    }
    
    #[test]
    fn test_classify_examples() {
        assert_eq!(classify_file(Path::new("examples/demo.rs")), FileCategory::Example);
    }
    
    #[test]
    fn test_module_path() {
        assert_eq!(compute_module_path(Path::new("src/lib.rs")), "crate");
        assert_eq!(compute_module_path(Path::new("src/foo.rs")), "crate::foo");
        assert_eq!(compute_module_path(Path::new("src/foo/mod.rs")), "crate::foo");
        assert_eq!(compute_module_path(Path::new("src/foo/bar.rs")), "crate::foo::bar");
    }
}
