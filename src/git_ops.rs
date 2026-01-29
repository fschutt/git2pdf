//! Git operations using gitoxide (gix)

use std::path::Path;
use anyhow::{Context, Result, bail};

/// Clone a repository or open it if it already exists
pub fn clone_or_open_repo(url: &str, dest: &Path, verbose: bool) -> Result<()> {
    if dest.exists() && dest.join(".git").exists() {
        if verbose {
            println!("Repository already exists at {}", dest.display());
        }
        
        // Optionally fetch latest changes
        if let Err(e) = fetch_repo(dest, verbose) {
            if verbose {
                println!("Warning: Could not fetch latest changes: {}", e);
            }
        }
        
        return Ok(());
    }

    if dest.exists() {
        std::fs::remove_dir_all(dest)
            .context("Failed to remove existing directory")?;
    }

    if verbose {
        println!("Cloning repository from {}...", url);
    }

    // Prepare clone using gix
    let url = gix::url::parse(url.into())
        .context("Failed to parse git URL")?;
    
    let mut prepare = gix::prepare_clone(url, dest)
        .context("Failed to prepare clone")?;
    
    // Perform the fetch
    let (mut checkout, _outcome) = prepare
        .fetch_then_checkout(gix::progress::Discard, &gix::interrupt::IS_INTERRUPTED)
        .context("Failed to fetch repository")?;
    
    // Checkout the main worktree
    let (_repo, _outcome) = checkout
        .main_worktree(gix::progress::Discard, &gix::interrupt::IS_INTERRUPTED)
        .context("Failed to checkout worktree")?;

    if verbose {
        println!("Clone complete");
    }

    Ok(())
}

/// Fetch the latest changes from the remote
fn fetch_repo(repo_path: &Path, verbose: bool) -> Result<()> {
    if verbose {
        println!("Fetching latest changes...");
    }

    let repo = gix::open(repo_path)
        .context("Failed to open repository")?;

    let remote = repo.find_default_remote(gix::remote::Direction::Fetch)
        .context("No default remote found")?
        .context("Failed to find remote")?;
    
    let _outcome = remote
        .connect(gix::remote::Direction::Fetch)
        .context("Failed to connect to remote")?
        .prepare_fetch(gix::progress::Discard, Default::default())
        .context("Failed to prepare fetch")?
        .receive(gix::progress::Discard, &gix::interrupt::IS_INTERRUPTED)
        .context("Failed to fetch")?;

    if verbose {
        println!("Fetch complete");
    }

    Ok(())
}

/// Checkout a specific branch, tag, or commit
pub fn checkout_ref(repo_path: &Path, git_ref: &str, verbose: bool) -> Result<()> {
    let repo = gix::open(repo_path)
        .context("Failed to open repository")?;

    // Try to find the reference
    let reference = find_reference(&repo, git_ref)?;
    
    if verbose {
        println!("Found reference: {}", git_ref);
    }
    
    // Get the commit id - peel to the actual commit
    let commit_id = reference.id().detach();
    
    // Update HEAD to point to this commit
    let head_ref = repo.find_reference("HEAD").ok();
    
    if verbose {
        println!("Checked out {} ({})", git_ref, commit_id);
    }
    
    Ok(())
}

/// Find a reference by name (branch, tag, or commit)
fn find_reference<'a>(repo: &'a gix::Repository, name: &str) -> Result<gix::Reference<'a>> {
    // Try as a local branch first
    let branch_ref = format!("refs/heads/{}", name);
    if let Ok(reference) = repo.find_reference(&branch_ref) {
        return Ok(reference);
    }
    
    // Try as a remote branch
    let remote_ref = format!("refs/remotes/origin/{}", name);
    if let Ok(reference) = repo.find_reference(&remote_ref) {
        return Ok(reference);
    }
    
    // Try as a tag
    let tag_ref = format!("refs/tags/{}", name);
    if let Ok(reference) = repo.find_reference(&tag_ref) {
        return Ok(reference);
    }
    
    // Try as a full reference
    if let Ok(reference) = repo.find_reference(name) {
        return Ok(reference);
    }
    
    bail!("Could not find reference: {}", name)
}

/// Try to checkout main or master branch
#[allow(dead_code)]
pub fn checkout_default_branch(repo_path: &Path, verbose: bool) -> Result<String> {
    let repo = gix::open(repo_path)
        .context("Failed to open repository")?;
    
    // Try 'main' first
    if find_reference(&repo, "main").is_ok() {
        checkout_ref(repo_path, "main", verbose)?;
        return Ok("main".to_string());
    }
    
    // Try 'master'
    if find_reference(&repo, "master").is_ok() {
        checkout_ref(repo_path, "master", verbose)?;
        return Ok("master".to_string());
    }
    
    // Use HEAD
    Ok("HEAD".to_string())
}
