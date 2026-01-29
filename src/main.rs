//! git2pdf - Convert git repositories to PDF for code review
//!
//! This tool clones a git repository (or uses a local path), discovers Rust crates,
//! classifies source files vs test files, generates syntax-highlighted HTML,
//! and converts them to PDF using printpdf's HTML layout engine.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use anyhow::{Context, Result, bail};
use clap::Parser;
use ignore::WalkBuilder;
use printpdf::{Base64OrRaw, GeneratePdfOptions, PdfDocument, PdfSaveOptions};
use syntect::highlighting::{Theme, ThemeSet};
use syntect::parsing::SyntaxSet;

mod crate_discovery;
mod file_classifier;
mod git_ops;
mod html_generator;

use crate_discovery::{CrateInfo, discover_crates};
use file_classifier::{classify_files, SourceFile, FileCategory};
use git_ops::{clone_or_open_repo, checkout_ref};
use html_generator::generate_html_for_crate;

/// git2pdf - Print git repositories to PDF for code review
#[derive(Parser, Debug)]
#[command(name = "git2pdf")]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Git repository URL or local file path
    #[arg(value_name = "SOURCE")]
    source: String,

    /// Branch, tag, or commit to checkout (default: tries 'main', then 'master')
    #[arg(short, long)]
    r#ref: Option<String>,

    /// Output directory for generated PDFs (default: current directory)
    #[arg(short, long, default_value = ".")]
    output: PathBuf,

    /// Paper size as WIDTHxHEIGHT in mm (default: 210x297 for A4)
    #[arg(long, default_value = "210x297")]
    paper_size: String,

    /// Margins in mm, CSS-style: "all", "vertical horizontal", or "top right bottom left" (default: 5)
    #[arg(long, default_value = "5")]
    margins: String,

    /// Font size in points for code
    #[arg(long, default_value = "6.0")]
    font_size: f32,

    /// Number of columns for code layout (currently not fully supported)
    #[arg(long, default_value = "1")]
    columns: u32,

    /// Include test files in output
    #[arg(long)]
    include_tests: bool,

    /// Syntax highlighting theme, or "none" to disable (default: InspiredGitHub)
    #[arg(long, default_value = "InspiredGitHub")]
    theme: String,

    /// Verbose output
    #[arg(short, long)]
    verbose: bool,

    /// Only process specific crates (comma-separated)
    #[arg(long)]
    crates: Option<String>,

    /// Temporary directory for cloning (default: system temp)
    #[arg(long)]
    temp_dir: Option<PathBuf>,

    /// Skip running cargo fmt on cloned repositories
    #[arg(long)]
    no_fmt: bool,

    /// Line width for rustfmt (default: 80)
    #[arg(long, default_value = "80")]
    line_width: u32,

    /// Path to a TTF font file to use for code (default: embedded RobotoMono-Bold)
    #[arg(long)]
    font: Option<PathBuf>,

    /// Start each source file on a new page
    #[arg(long)]
    page_break: bool,
}

/// Parse paper size from "WIDTHxHEIGHT" format (in mm)
fn parse_paper_size(s: &str) -> Result<(f32, f32)> {
    let parts: Vec<&str> = s.split('x').collect();
    if parts.len() != 2 {
        bail!("Invalid paper size format. Expected WIDTHxHEIGHT (e.g., 210x297)");
    }
    let width: f32 = parts[0].trim().parse()
        .context("Invalid paper width")?;
    let height: f32 = parts[1].trim().parse()
        .context("Invalid paper height")?;
    Ok((width, height))
}

/// Parse margins from CSS-style format (in mm)
/// Accepts: "all", "vertical horizontal", or "top right bottom left"
fn parse_margins(s: &str) -> Result<(f32, f32, f32, f32)> {
    let parts: Vec<f32> = s.split_whitespace()
        .map(|p| p.trim().parse::<f32>())
        .collect::<std::result::Result<Vec<_>, _>>()
        .context("Invalid margin value")?;
    
    match parts.len() {
        1 => Ok((parts[0], parts[0], parts[0], parts[0])),
        2 => Ok((parts[0], parts[1], parts[0], parts[1])), // vertical, horizontal
        4 => Ok((parts[0], parts[1], parts[2], parts[3])), // top, right, bottom, left
        _ => bail!("Invalid margins format. Expected 1, 2, or 4 values (e.g., \"10\", \"10 20\", or \"10 20 10 20\")"),
    }
}

fn main() -> Result<()> {
    let args = Args::parse();
    let start = Instant::now();

    // Parse paper size
    let (paper_width, paper_height) = parse_paper_size(&args.paper_size)?;
    
    // Parse margins (top, right, bottom, left)
    let (margin_top, margin_right, margin_bottom, margin_left) = parse_margins(&args.margins)?;

    if args.verbose {
        println!("[{:?}] git2pdf - Converting repository to PDF", start.elapsed());
        println!("[{:?}] Source: {}", start.elapsed(), args.source);
        println!("[{:?}] Paper size: {}x{} mm", start.elapsed(), paper_width, paper_height);
        println!("[{:?}] Margins: top={}, right={}, bottom={}, left={} mm", 
                 start.elapsed(), margin_top, margin_right, margin_bottom, margin_left);
    }

    // Determine if source is a URL or local path
    let is_remote = args.source.starts_with("http://") 
        || args.source.starts_with("https://") 
        || args.source.starts_with("git@") 
        || args.source.starts_with("ssh://");

    // Setup temp directory
    let temp_dir = args.temp_dir.clone().unwrap_or_else(|| {
        std::env::temp_dir().join("git2pdf")
    });
    fs::create_dir_all(&temp_dir)?;

    // Get source path (clone if remote, use directly if local)
    let source_path = if is_remote {
        let repo_name = extract_repo_name(&args.source)?;
        let clone_path = temp_dir.join(&repo_name);
        
        if args.verbose {
            println!("[{:?}] Cloning to: {}", start.elapsed(), clone_path.display());
        }
        
        clone_or_open_repo(&args.source, &clone_path, args.verbose)?;
        
        // Checkout the specified ref if provided
        if let Some(ref git_ref) = args.r#ref {
            if args.verbose {
                println!("[{:?}] Checking out: {}", start.elapsed(), git_ref);
            }
            checkout_ref(&clone_path, git_ref, args.verbose)?;
        }
        
        clone_path
    } else {
        let local_path = PathBuf::from(&args.source);
        if !local_path.exists() {
            bail!("Repository path does not exist: {}", local_path.display());
        }
        
        // Checkout the specified ref if provided (for local repos)
        if let Some(ref git_ref) = args.r#ref {
            if args.verbose {
                println!("[{:?}] Checking out: {}", start.elapsed(), git_ref);
            }
            checkout_ref(&local_path, git_ref, args.verbose)?;
        }
        
        local_path
    };

    // Copy files to work directory (respecting .gitignore)
    // For remote repos, we already have them in temp_dir, so just use that
    // For local repos, copy to temp to avoid modifying original
    let work_dir = if is_remote {
        source_path.clone()
    } else {
        let repo_name = source_path.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "repo".to_string());
        let work_path = temp_dir.join(format!("{}-work", repo_name));
        
        if args.verbose {
            println!("[{:?}] Copying files to work directory: {}", start.elapsed(), work_path.display());
        }
        
        copy_repo_files(&source_path, &work_path, args.verbose)?;
        work_path
    };

    // Run cargo fmt on work directory (unless disabled)
    if !args.no_fmt {
        if args.verbose {
            println!("[{:?}] Running cargo fmt with line width {}...", start.elapsed(), args.line_width);
        }
        run_cargo_fmt(&work_dir, args.line_width, args.verbose)?;
    }

    // Discover crates in the repository
    if args.verbose {
        println!("[{:?}] Discovering crates...", start.elapsed());
    }
    let crates = discover_crates(&work_dir)?;
    
    if crates.is_empty() {
        bail!("No Rust crates found in repository");
    }

    if args.verbose {
        println!("[{:?}] Found {} crate(s):", start.elapsed(), crates.len());
        for c in &crates {
            println!("  - {} ({})", c.name, c.path.display());
        }
    }

    // Filter crates if specified
    let crates_to_process: Vec<&CrateInfo> = if let Some(ref filter) = args.crates {
        let filter_names: Vec<&str> = filter.split(',').map(|s| s.trim()).collect();
        crates.iter()
            .filter(|c| filter_names.contains(&c.name.as_str()))
            .collect()
    } else {
        crates.iter().collect()
    };

    if crates_to_process.is_empty() {
        bail!("No crates matched the filter");
    }

    // Create output directory
    fs::create_dir_all(&args.output)?;

    // Load syntax highlighting (None if theme is "none")
    if args.verbose {
        println!("[{:?}] Loading syntax highlighting...", start.elapsed());
    }
    let syntax_set = SyntaxSet::load_defaults_newlines();
    let theme_set = ThemeSet::load_defaults();
    let theme: Option<&Theme> = if args.theme.to_lowercase() == "none" {
        None
    } else {
        Some(theme_set.themes.get(&args.theme)
            .or_else(|| theme_set.themes.get("InspiredGitHub"))
            .context("Failed to load syntax theme")?)
    };

    // Process each crate
    for crate_info in crates_to_process {
        if args.verbose {
            println!("\n[{:?}] Processing crate: {}", start.elapsed(), crate_info.name);
        }

        // Classify files
        let files = classify_files(&crate_info.path, args.include_tests)?;
        
        let source_files: Vec<&SourceFile> = files.iter()
            .filter(|f| f.category == FileCategory::Source || 
                       (args.include_tests && matches!(f.category, FileCategory::Test | FileCategory::IntegrationTest)))
            .collect();

        if source_files.is_empty() {
            if args.verbose {
                println!("  No source files found, skipping");
            }
            continue;
        }

        if args.verbose {
            println!("  Found {} source file(s)", source_files.len());
        }

        // Generate HTML
        let html = generate_html_for_crate(
            crate_info,
            &source_files,
            &syntax_set,
            theme,
            args.font_size,
            args.columns,
            args.page_break,
        )?;

        // Optionally save HTML for debugging
        if args.verbose {
            let html_path = args.output.join(format!("{}.html", crate_info.name));
            fs::write(&html_path, &html)?;
            println!("  Saved HTML: {}", html_path.display());
        }

        // Generate PDF
        let options = GeneratePdfOptions {
            page_width: Some(paper_width),
            page_height: Some(paper_height),
            margin_top: Some(margin_top),
            margin_right: Some(margin_right),
            margin_bottom: Some(margin_bottom),
            margin_left: Some(margin_left),
            show_page_numbers: Some(false),
            ..Default::default()
        };

        let images = BTreeMap::new();
        
        // Load custom font or use embedded RobotoMono-Bold
        let mut fonts: BTreeMap<String, Base64OrRaw> = BTreeMap::new();
        let font_bytes: Vec<u8> = if let Some(ref font_path) = args.font {
            fs::read(font_path)
                .with_context(|| format!("Failed to read font file: {}", font_path.display()))?
        } else {
            include_bytes!("../fonts/RobotoMono-Bold.ttf").to_vec()
        };
        fonts.insert("RobotoMono".to_string(), Base64OrRaw::Raw(font_bytes));
        
        let mut warnings = Vec::new();

        let doc = PdfDocument::from_html(&html, &images, &fonts, &options, &mut warnings)
            .map_err(|e| anyhow::anyhow!("Failed to generate PDF: {}", e))?;

        if args.verbose && !warnings.is_empty() {
            println!("  PDF generation warnings: {}", warnings.len());
        }

        // Save PDF
        let output_path = args.output.join(format!("{}.pdf", crate_info.name));
        let save_options = PdfSaveOptions::default();
        let mut save_warnings = Vec::new();
        let bytes = doc.save(&save_options, &mut save_warnings);

        fs::write(&output_path, bytes)?;
        println!("Created: {}", output_path.display());
    }

    println!("\nDone!");
    Ok(())
}

/// Extract repository name from URL
fn extract_repo_name(url: &str) -> Result<String> {
    // Handle various URL formats:
    // https://github.com/user/repo.git
    // git@github.com:user/repo.git
    // ssh://git@github.com/user/repo.git
    
    let url = url.trim_end_matches(".git");
    
    if let Some(name) = url.rsplit('/').next() {
        if !name.is_empty() {
            return Ok(name.to_string());
        }
    }
    
    // Try git@ format
    if let Some(path) = url.split(':').last() {
        if let Some(name) = path.rsplit('/').next() {
            if !name.is_empty() {
                return Ok(name.to_string());
            }
        }
    }
    
    bail!("Could not extract repository name from URL: {}", url)
}

/// Run cargo fmt on a repository with specified line width
fn run_cargo_fmt(repo_path: &Path, line_width: u32, verbose: bool) -> Result<()> {
    // Create a rustfmt.toml with the specified line width
    let rustfmt_config = format!("max_width = {}\n", line_width);
    let rustfmt_path = repo_path.join("rustfmt.toml");
    
    // Only write if it doesn't exist (don't override existing config)
    if !rustfmt_path.exists() {
        fs::write(&rustfmt_path, &rustfmt_config)?;
    }
    
    let output = Command::new("cargo")
        .arg("fmt")
        .current_dir(repo_path)
        .output()
        .context("Failed to run cargo fmt. Is cargo installed?")?;
    
    if verbose {
        if !output.stdout.is_empty() {
            println!("  cargo fmt stdout: {}", String::from_utf8_lossy(&output.stdout));
        }
        if !output.stderr.is_empty() {
            println!("  cargo fmt stderr: {}", String::from_utf8_lossy(&output.stderr));
        }
    }
    
    // Don't fail if cargo fmt fails (repo might not be a valid Rust project)
    if !output.status.success() && verbose {
        println!("  Warning: cargo fmt exited with non-zero status");
    }
    
    Ok(())
}

/// Copy repository files to destination, respecting .gitignore
fn copy_repo_files(src: &Path, dst: &Path, verbose: bool) -> Result<()> {
    // Remove destination if it exists
    if dst.exists() {
        fs::remove_dir_all(dst)?;
    }
    fs::create_dir_all(dst)?;
    
    // Use ignore crate to walk files respecting .gitignore
    let walker = WalkBuilder::new(src)
        .hidden(false)           // Include hidden files (like .gitignore itself)
        .git_ignore(true)        // Respect .gitignore
        .git_global(true)        // Respect global gitignore
        .git_exclude(true)       // Respect .git/info/exclude
        .build();
    
    let mut file_count = 0;
    for entry in walker {
        let entry = entry?;
        let path = entry.path();
        
        // Skip the .git directory
        if path.components().any(|c| c.as_os_str() == ".git") {
            continue;
        }
        
        // Get relative path
        let rel_path = path.strip_prefix(src).unwrap_or(path);
        let dst_path = dst.join(rel_path);
        
        if path.is_dir() {
            fs::create_dir_all(&dst_path)?;
        } else if path.is_file() {
            if let Some(parent) = dst_path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(path, &dst_path)?;
            file_count += 1;
        }
    }
    
    if verbose {
        println!("  Copied {} files", file_count);
    }
    
    Ok(())
}
