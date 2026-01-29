//! git2pdf - Convert git repositories to PDF for code review
//!
//! This tool clones a git repository (or uses a local path), discovers Rust crates,
//! classifies source files vs test files, generates syntax-highlighted HTML,
//! and converts them to PDF using printpdf's HTML layout engine.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use clap::Parser;
use printpdf::{GeneratePdfOptions, PdfDocument, PdfSaveOptions};
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

    /// Margins in mm, CSS-style: "all", "vertical horizontal", or "top right bottom left" (default: 10)
    #[arg(long, default_value = "10")]
    margins: String,

    /// Font size in points for code
    #[arg(long, default_value = "8.0")]
    font_size: f32,

    /// Number of columns for code layout (default: 2)
    #[arg(long, default_value = "2")]
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

    // Parse paper size
    let (paper_width, paper_height) = parse_paper_size(&args.paper_size)?;
    
    // Parse margins (top, right, bottom, left)
    let (margin_top, margin_right, margin_bottom, margin_left) = parse_margins(&args.margins)?;

    if args.verbose {
        println!("git2pdf - Converting repository to PDF");
        println!("Source: {}", args.source);
        println!("Paper size: {}x{} mm", paper_width, paper_height);
        println!("Margins: top={}, right={}, bottom={}, left={} mm", 
                 margin_top, margin_right, margin_bottom, margin_left);
    }

    // Determine if source is a URL or local path
    let repo_path = if args.source.starts_with("http://") 
        || args.source.starts_with("https://") 
        || args.source.starts_with("git@") 
        || args.source.starts_with("ssh://")
    {
        // Clone the repository
        let temp_dir = args.temp_dir.clone().unwrap_or_else(|| {
            std::env::temp_dir().join("git2pdf")
        });
        fs::create_dir_all(&temp_dir)?;
        
        let repo_name = extract_repo_name(&args.source)?;
        let clone_path = temp_dir.join(&repo_name);
        
        if args.verbose {
            println!("Cloning to: {}", clone_path.display());
        }
        
        clone_or_open_repo(&args.source, &clone_path, args.verbose)?;
        clone_path
    } else {
        // Use local path
        PathBuf::from(&args.source)
    };

    if !repo_path.exists() {
        bail!("Repository path does not exist: {}", repo_path.display());
    }

    // Checkout the specified ref if provided
    if let Some(ref git_ref) = args.r#ref {
        if args.verbose {
            println!("Checking out: {}", git_ref);
        }
        checkout_ref(&repo_path, git_ref, args.verbose)?;
    }

    // Discover crates in the repository
    if args.verbose {
        println!("Discovering crates...");
    }
    let crates = discover_crates(&repo_path)?;
    
    if crates.is_empty() {
        bail!("No Rust crates found in repository");
    }

    if args.verbose {
        println!("Found {} crate(s):", crates.len());
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
            println!("\nProcessing crate: {}", crate_info.name);
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
        )?;

        // Generate PDF
        let options = GeneratePdfOptions {
            page_width: Some(paper_width),
            page_height: Some(paper_height),
            margin_top: Some(margin_top),
            margin_right: Some(margin_right),
            margin_bottom: Some(margin_bottom),
            margin_left: Some(margin_left),
            show_page_numbers: Some(true),
            header_text: Some(format!("{} - Code Review", crate_info.name)),
            ..Default::default()
        };

        let images = BTreeMap::new();
        let fonts = BTreeMap::new();
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
