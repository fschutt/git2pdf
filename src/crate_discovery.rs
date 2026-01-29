//! Rust crate discovery in a repository

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;

/// Information about a discovered Rust crate
#[derive(Debug, Clone)]
pub struct CrateInfo {
    /// Crate name from Cargo.toml
    pub name: String,
    /// Path to the crate root (directory containing Cargo.toml)
    pub path: PathBuf,
    /// Whether this is a workspace member
    pub is_workspace_member: bool,
    /// Crate version
    pub version: String,
    /// Crate description
    pub description: Option<String>,
}

/// Minimal Cargo.toml structure for parsing
#[derive(Debug, Deserialize)]
struct CargoToml {
    package: Option<Package>,
    workspace: Option<Workspace>,
}

#[derive(Debug, Deserialize)]
struct Package {
    name: String,
    #[serde(default = "default_version")]
    version: String,
    description: Option<String>,
}

fn default_version() -> String {
    "0.0.0".to_string()
}

#[derive(Debug, Deserialize)]
struct Workspace {
    members: Option<Vec<String>>,
    #[serde(default)]
    exclude: Vec<String>,
}

/// Discover all Rust crates in a repository
pub fn discover_crates(repo_path: &Path) -> Result<Vec<CrateInfo>> {
    let mut crates = Vec::new();
    
    // Check if there's a root Cargo.toml
    let root_cargo = repo_path.join("Cargo.toml");
    
    if !root_cargo.exists() {
        // No Cargo.toml at root, search recursively
        return discover_crates_recursive(repo_path);
    }
    
    let content = fs::read_to_string(&root_cargo)
        .context("Failed to read root Cargo.toml")?;
    
    let cargo_toml: CargoToml = toml::from_str(&content)
        .context("Failed to parse root Cargo.toml")?;
    
    // Check if it's a workspace
    if let Some(workspace) = cargo_toml.workspace {
        // It's a workspace - discover members
        if let Some(members) = workspace.members {
            for member_pattern in members {
                let member_crates = expand_workspace_member(repo_path, &member_pattern, &workspace.exclude)?;
                crates.extend(member_crates);
            }
        }
        
        // Also check if the root is a package
        if let Some(package) = cargo_toml.package {
            crates.push(CrateInfo {
                name: package.name,
                path: repo_path.to_path_buf(),
                is_workspace_member: false,
                version: package.version,
                description: package.description,
            });
        }
    } else if let Some(package) = cargo_toml.package {
        // It's a single crate
        crates.push(CrateInfo {
            name: package.name,
            path: repo_path.to_path_buf(),
            is_workspace_member: false,
            version: package.version,
            description: package.description,
        });
    }
    
    // Sort by name for consistent output
    crates.sort_by(|a, b| a.name.cmp(&b.name));
    
    // Remove duplicates (by path)
    crates.dedup_by(|a, b| a.path == b.path);
    
    Ok(crates)
}

/// Expand a workspace member pattern (supports glob patterns like "crates/*")
fn expand_workspace_member(
    repo_path: &Path,
    pattern: &str,
    exclude: &[String],
) -> Result<Vec<CrateInfo>> {
    let mut crates = Vec::new();
    
    if pattern.contains('*') {
        // It's a glob pattern
        let base_path = repo_path.join(pattern.split('*').next().unwrap_or(""));
        
        if base_path.exists() && base_path.is_dir() {
            for entry in fs::read_dir(&base_path)? {
                let entry = entry?;
                let path = entry.path();
                
                if path.is_dir() {
                    // Check if excluded
                    let rel_path = path.strip_prefix(repo_path)
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or_default();
                    
                    if exclude.iter().any(|e| rel_path.starts_with(e)) {
                        continue;
                    }
                    
                    if let Some(crate_info) = try_parse_crate(&path)? {
                        crates.push(CrateInfo {
                            is_workspace_member: true,
                            ..crate_info
                        });
                    }
                }
            }
        }
    } else {
        // Exact path
        let member_path = repo_path.join(pattern);
        
        // Check if excluded
        if exclude.iter().any(|e| pattern.starts_with(e)) {
            return Ok(crates);
        }
        
        if let Some(crate_info) = try_parse_crate(&member_path)? {
            crates.push(CrateInfo {
                is_workspace_member: true,
                ..crate_info
            });
        }
    }
    
    Ok(crates)
}

/// Try to parse a crate from a directory
fn try_parse_crate(path: &Path) -> Result<Option<CrateInfo>> {
    let cargo_path = path.join("Cargo.toml");
    
    if !cargo_path.exists() {
        return Ok(None);
    }
    
    let content = fs::read_to_string(&cargo_path)
        .context("Failed to read Cargo.toml")?;
    
    let cargo_toml: CargoToml = toml::from_str(&content)
        .context("Failed to parse Cargo.toml")?;
    
    if let Some(package) = cargo_toml.package {
        Ok(Some(CrateInfo {
            name: package.name,
            path: path.to_path_buf(),
            is_workspace_member: false,
            version: package.version,
            description: package.description,
        }))
    } else {
        Ok(None)
    }
}

/// Recursively discover crates when there's no workspace, respecting .gitignore
fn discover_crates_recursive(repo_path: &Path) -> Result<Vec<CrateInfo>> {
    use ignore::WalkBuilder;
    
    let mut crates = Vec::new();
    
    let walker = WalkBuilder::new(repo_path)
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
        
        // Skip target and node_modules explicitly
        if path.components().any(|c| {
            let s = c.as_os_str().to_string_lossy();
            s == "target" || s == "node_modules"
        }) {
            continue;
        }
        
        if path.file_name().map(|n| n == "Cargo.toml").unwrap_or(false) {
            let parent = path.parent().unwrap_or(repo_path);
            if let Some(crate_info) = try_parse_crate(parent)? {
                crates.push(crate_info);
            }
        }
    }
    
    // Sort by name
    crates.sort_by(|a, b| a.name.cmp(&b.name));
    
    Ok(crates)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_discover_crates_single() {
        // This would need a test fixture
    }
}
