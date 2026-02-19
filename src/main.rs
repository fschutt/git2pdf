//! git2pdf - Convert git repositories to PDF for code review
//!
//! This tool clones a git repository (or uses a local path), discovers Rust crates,
//! classifies source files vs test files, generates syntax-highlighted HTML,
//! and converts them to PDF using printpdf's HTML layout engine.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::Instant;

use anyhow::{Context, Result, bail};
use clap::Parser;
use ignore::WalkBuilder;
use printpdf::{Base64OrRaw, GeneratePdfOptions, PdfDocument, PdfParseOptions, PdfSaveOptions};
use syntect::highlighting::{Theme, ThemeSet};
use syntect::parsing::SyntaxSet;

mod crate_discovery;
mod file_classifier;
mod git_ops;
mod html_generator;

use crate_discovery::{CrateInfo, discover_crates};
use file_classifier::{classify_files, SourceFile, FileCategory};
use git_ops::{clone_or_open_repo, checkout_ref, get_git_hash};
use html_generator::{generate_html_for_single_file, generate_title_page_html};

/// git2pdf - Print git repositories to PDF for code review
#[derive(Parser, Debug)]
#[command(name = "git2pdf")]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Git repository URL or local file path
    #[arg(value_name = "SOURCE", required_unless_present = "file")]
    source: Option<String>,

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

    /// Process files in parallel using rayon
    #[arg(long)]
    parallel: bool,

    /// Process a single file directly (bypasses git/crate logic, for benchmarking)
    #[arg(long)]
    file: Option<PathBuf>,
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

    // Configure rayon thread pool to use n-1 cores (leave one core free for OS)
    if args.parallel {
        let num_cpus = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1);
        let num_threads = num_cpus.saturating_sub(1).max(1);
        rayon::ThreadPoolBuilder::new()
            .num_threads(num_threads)
            .build_global()
            .ok(); // ignore error if already initialized
        if args.verbose {
            println!("[{:?}] Parallel mode: using {} of {} cores", start.elapsed(), num_threads, num_cpus);
        }
    }
    
    // Parse paper size
    let (paper_width, paper_height) = parse_paper_size(&args.paper_size)?;
    
    // Parse margins (top, right, bottom, left)
    let (margin_top, margin_right, margin_bottom, margin_left) = parse_margins(&args.margins)?;

    if args.verbose {
        println!("[{:?}] git2pdf - Converting repository to PDF", start.elapsed());
        if let Some(ref s) = args.source {
            println!("[{:?}] Source: {}", start.elapsed(), s);
        }
        println!("[{:?}] Paper size: {}x{} mm", start.elapsed(), paper_width, paper_height);
        println!("[{:?}] Margins: top={}, right={}, bottom={}, left={} mm", 
                 start.elapsed(), margin_top, margin_right, margin_bottom, margin_left);
    }

    // Single-file mode: bypass all git/crate logic
    if let Some(ref file_path) = args.file {
        return process_single_file(
            file_path,
            &args,
            paper_width, paper_height,
            margin_top, margin_right, margin_bottom, margin_left,
        );
    }

    // From here on, source is required (guaranteed by clap's required_unless_present)
    let source = args.source.as_ref().unwrap();

    // Determine if source is a URL or local path
    let is_remote = source.starts_with("http://") 
        || source.starts_with("https://") 
        || source.starts_with("git@") 
        || source.starts_with("ssh://");

    // Setup temp directory
    let temp_dir = args.temp_dir.clone().unwrap_or_else(|| {
        std::env::temp_dir().join("git2pdf")
    });
    fs::create_dir_all(&temp_dir)?;

    // Get source path (clone if remote, use directly if local)
    let source_path = if is_remote {
        let repo_name = extract_repo_name(&source)?;
        let clone_path = temp_dir.join(&repo_name);
        
        if args.verbose {
            println!("[{:?}] Cloning to: {}", start.elapsed(), clone_path.display());
        }
        
        clone_or_open_repo(&source, &clone_path, args.verbose)?;
        
        // Checkout the specified ref if provided
        if let Some(ref git_ref) = args.r#ref {
            if args.verbose {
                println!("[{:?}] Checking out: {}", start.elapsed(), git_ref);
            }
            checkout_ref(&clone_path, git_ref, args.verbose)?;
        }
        
        clone_path
    } else {
        let local_path = PathBuf::from(&*source);
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

    // Discover crates in the repository
    if args.verbose {
        println!("[{:?}] Discovering crates...", start.elapsed());
    }
    let crates = discover_crates(&work_dir)?;

    // Run cargo fmt per-crate (unless disabled)
    // We format per-crate instead of the whole workspace because submodule
    // dependencies (e.g. webrender) may not be present in the work directory,
    // which would cause `cargo fmt` on the root workspace to fail.
    if !args.no_fmt {
        if args.verbose {
            println!("[{:?}] Running cargo fmt with line width {}...", start.elapsed(), args.line_width);
        }
        for c in &crates {
            run_cargo_fmt(&c.path, args.line_width, args.verbose)?;
        }
    }
    
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

    // Load syntax highlighting
    if args.verbose {
        println!("[{:?}] Loading syntax highlighting...", start.elapsed());
    }

    // Get git hash for title pages
    let git_hash = get_git_hash(&work_dir).ok();

    // Load font bytes once (shared across all parallel tasks)
    let font_bytes: Arc<Vec<u8>> = Arc::new(if let Some(ref font_path) = args.font {
        fs::read(font_path)
            .with_context(|| format!("Failed to read font file: {}", font_path.display()))?
    } else {
        include_bytes!("../fonts/RobotoMono-Bold.ttf").to_vec()
    });

    // Wrap syntax_set and theme_set in Arc for sharing across threads
    let syntax_set = Arc::new(SyntaxSet::load_defaults_newlines());
    let theme_set = Arc::new(ThemeSet::load_defaults());

    // PDF generation options (shared)
    let pdf_options = GeneratePdfOptions {
        page_width: Some(paper_width),
        page_height: Some(paper_height),
        margin_top: Some(margin_top),
        margin_right: Some(margin_right),
        margin_bottom: Some(margin_bottom),
        margin_left: Some(margin_left),
        show_page_numbers: Some(false),
        ..Default::default()
    };

    // Process each crate
    for crate_info in crates_to_process {
        if args.verbose {
            println!("\n[{:?}] Processing crate: {}", start.elapsed(), crate_info.name);
        }

        // Classify files
        let files = classify_files(&crate_info.path, args.include_tests)?;
        
        let source_files: Vec<SourceFile> = files.into_iter()
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
            println!("  Found {} source file(s), processing in parallel...", source_files.len());
        }

        // Create fonts map for PDF generation
        let mut fonts: BTreeMap<String, Base64OrRaw> = BTreeMap::new();
        fonts.insert("RobotoMono".to_string(), Base64OrRaw::Raw((*font_bytes).clone()));
        
        // Build font pool ONCE and share across all from_html calls.
        let fc_cache_start = Instant::now();
        let raw_fonts: BTreeMap<String, Vec<u8>> = fonts.iter().map(|(k, v)| {
            let bytes = match v {
                Base64OrRaw::Raw(b) => b.clone(),
                Base64OrRaw::B64(_) => Vec::new(),
            };
            (k.clone(), bytes)
        }).collect();
        let font_pool = printpdf::html::build_font_pool(
            &raw_fonts,
            Some(&["monospace"]),
        );
        if args.verbose {
            println!("  Font pool built in {:?} (shared across all files)", fc_cache_start.elapsed());
        }

        // Phase 1: Render each source file to an individual PDF on disk.
        // This avoids holding all PdfDocuments in memory at once (OOM on large crates).
        let theme_name = args.theme.clone();
        let font_size = args.font_size;
        let pdf_opts = pdf_options.clone();
        let font_bytes_clone = Arc::clone(&font_bytes);
        let syntax_set_clone = Arc::clone(&syntax_set);
        let theme_set_clone = Arc::clone(&theme_set);
        let font_pool_clone = font_pool.clone();

        let cache_dir = temp_dir.join(format!("{}-cache", crate_info.name));
        fs::create_dir_all(&cache_dir)?;

        let process_file = |file: &SourceFile| -> Result<(String, PathBuf, usize, std::time::Duration)> {
            let file_start = std::time::Instant::now();
            let theme: Option<&Theme> = if theme_name.to_lowercase() == "none" {
                None
            } else {
                theme_set_clone.themes.get(&theme_name)
                    .or_else(|| theme_set_clone.themes.get("InspiredGitHub"))
            };

            let loc = std::fs::read_to_string(&file.path)
                .map(|s| s.lines().count())
                .unwrap_or(0);

            let html_start = std::time::Instant::now();
            let html = generate_html_for_single_file(file, &syntax_set_clone, theme, font_size)?;
            let html_elapsed = html_start.elapsed();

            let mut file_fonts: BTreeMap<String, Base64OrRaw> = BTreeMap::new();
            file_fonts.insert("RobotoMono".to_string(), Base64OrRaw::Raw((*font_bytes_clone).clone()));

            let pdf_start = std::time::Instant::now();
            let mut warnings = Vec::new();
            let doc = PdfDocument::from_html_with_cache(
                &html, &BTreeMap::new(), &file_fonts, &pdf_opts, &mut warnings,
                Some(font_pool_clone.clone()),
            ).map_err(|e| anyhow::anyhow!("Failed to generate PDF for {}: {}", file.relative_path.display(), e))?;
            let pdf_elapsed = pdf_start.elapsed();

            // Save to disk immediately, then drop to free memory
            let safe_name = file.relative_path.to_string_lossy()
                .replace('/', "__")
                .replace('\\', "__");
            let cache_path = cache_dir.join(format!("{}.pdf", safe_name));
            {
                let save_options = PdfSaveOptions::default();
                let mut save_warnings = Vec::new();
                let bytes = doc.save(&save_options, &mut save_warnings);
                fs::write(&cache_path, bytes)?;
            }

            eprintln!("    [detail] {} ({} LOC, {} bytes HTML): html_gen={:.1?}, pdf_render={:.1?}",
                file.relative_path.display(), loc, html.len(), html_elapsed, pdf_elapsed);

            Ok((file.relative_path.to_string_lossy().to_string(), cache_path, loc, file_start.elapsed()))
        };

        let file_results: Vec<Result<(String, PathBuf, usize, std::time::Duration)>> = if args.parallel {
            use rayon::prelude::*;
            source_files.par_iter().map(process_file).collect()
        } else {
            source_files.iter().map(process_file).collect()
        };

        // Collect successful results (preserving source file order)
        let mut cached_files: Vec<(String, PathBuf, usize, std::time::Duration)> = Vec::new();
        for result in file_results {
            match result {
                Ok(info) => cached_files.push(info),
                Err(e) => eprintln!("  Warning: {}", e),
            }
        }

        // Phase 2: Generate title page in-memory, then append each cached file PDF one by one.
        let title_html = generate_title_page_html(crate_info, git_hash.as_deref(), args.font_size);
        let mut title_warnings = Vec::new();
        let mut combined_doc = PdfDocument::from_html_with_cache(
            &title_html, &BTreeMap::new(), &fonts, &pdf_options, &mut title_warnings,
            Some(font_pool.clone()),
        ).map_err(|e| anyhow::anyhow!("Failed to generate title page: {}", e))?;

        if args.verbose {
            println!("  Title page: {} page(s). Appending {} file PDFs...", combined_doc.page_count(), cached_files.len());
        }

        let mut file_count = 0;
        for (path, cache_path, loc, elapsed) in &cached_files {
            let file_bytes = fs::read(cache_path)?;
            let file_doc = PdfDocument::parse(
                &file_bytes, &PdfParseOptions::default(), &mut Vec::new(),
            ).map_err(|e| anyhow::anyhow!("Failed to reload {}: {}", path, e))?;
            drop(file_bytes);
            combined_doc.append_document(file_doc);
            file_count += 1;
            if args.verbose {
                println!("  Added: {} ({} LOC, {} pages total, {:.1?})", path, loc, combined_doc.page_count(), elapsed);
            }
        }

        if args.verbose {
            println!("  Combined {} files into {} pages", file_count, combined_doc.page_count());
        }

        // Save final PDF
        let output_path = args.output.join(format!("{}.pdf", crate_info.name));
        let save_options = PdfSaveOptions::default();
        let mut save_warnings = Vec::new();
        let bytes = combined_doc.save(&save_options, &mut save_warnings);
        fs::write(&output_path, bytes)?;
        println!("Created: {} ({} pages)", output_path.display(), combined_doc.page_count());

        // Clean up cache directory
        let _ = fs::remove_dir_all(&cache_dir);
    }

    println!("\nDone in {:?}!", start.elapsed());
    Ok(())
}

/// Process a single file directly — bypasses git/crate discovery.
/// Useful for benchmarking layout performance on files of varying size.
fn process_single_file(
    file_path: &Path,
    args: &Args,
    paper_width: f32, paper_height: f32,
    margin_top: f32, margin_right: f32, margin_bottom: f32, margin_left: f32,
) -> Result<()> {
    use std::time::Instant;

    if !file_path.exists() {
        bail!("File not found: {}", file_path.display());
    }

    let total_start = Instant::now();
    let file_name = file_path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "file".to_string());

    // Read file content and count LOC
    let content = fs::read_to_string(file_path)
        .with_context(|| format!("Failed to read: {}", file_path.display()))?;
    let loc = content.lines().count();
    let content_bytes = content.len();
    eprintln!("[timing] file={}, LOC={}, bytes={}", file_name, loc, content_bytes);

    // Setup syntax highlighting
    let t0 = Instant::now();
    let syntax_set = SyntaxSet::load_defaults_newlines();
    let theme_set = ThemeSet::load_defaults();
    let theme: Option<&Theme> = if args.theme.to_lowercase() == "none" {
        None
    } else {
        theme_set.themes.get(&args.theme)
            .or_else(|| theme_set.themes.get("InspiredGitHub"))
    };
    eprintln!("[timing] syntax_load: {:.1?}", t0.elapsed());

    // Create SourceFile struct
    let source_file = SourceFile {
        path: file_path.to_path_buf(),
        relative_path: PathBuf::from(&file_name),
        category: FileCategory::Source,
        module_path: String::new(),
    };

    // Generate HTML
    let t1 = Instant::now();
    let html = generate_html_for_single_file(&source_file, &syntax_set, theme, args.font_size)?;
    let html_elapsed = t1.elapsed();
    eprintln!("[timing] html_generation: {:.1?} ({} bytes HTML)", html_elapsed, html.len());

    // Setup fonts
    let t2 = Instant::now();
    let font_bytes: Vec<u8> = if let Some(ref font_path) = args.font {
        fs::read(font_path)?
    } else {
        include_bytes!("../fonts/RobotoMono-Bold.ttf").to_vec()
    };
    let mut fonts: BTreeMap<String, Base64OrRaw> = BTreeMap::new();
    fonts.insert("RobotoMono".to_string(), Base64OrRaw::Raw(font_bytes.clone()));

    let raw_fonts: BTreeMap<String, Vec<u8>> = fonts.iter().map(|(k, v)| {
        let bytes = match v {
            Base64OrRaw::Raw(b) => b.clone(),
            Base64OrRaw::B64(_) => Vec::new(),
        };
        (k.clone(), bytes)
    }).collect();
    let font_pool = printpdf::html::build_font_pool(&raw_fonts, Some(&["monospace"]));
    let font_elapsed = t2.elapsed();
    eprintln!("[timing] font_pool_build: {:.1?}", font_elapsed);

    // PDF generation options
    let pdf_options = GeneratePdfOptions {
        page_width: Some(paper_width),
        page_height: Some(paper_height),
        margin_top: Some(margin_top),
        margin_right: Some(margin_right),
        margin_bottom: Some(margin_bottom),
        margin_left: Some(margin_left),
        show_page_numbers: Some(false),
        ..Default::default()
    };

    // Generate PDF (the main bottleneck we're benchmarking)
    let t3 = Instant::now();
    let mut warnings = Vec::new();
    let doc = PdfDocument::from_html_with_cache(
        &html, &BTreeMap::new(), &fonts, &pdf_options, &mut warnings,
        Some(font_pool),
    ).map_err(|e| anyhow::anyhow!("Failed to generate PDF: {}", e))?;
    let pdf_elapsed = t3.elapsed();
    let pages = doc.page_count();
    eprintln!("[timing] pdf_render: {:.1?} ({} pages)", pdf_elapsed, pages);

    // Save PDF
    let t4 = Instant::now();
    let output_path = args.output.join(format!("{}.pdf", file_name.trim_end_matches(".rs")));
    let save_options = PdfSaveOptions::default();
    let mut save_warnings = Vec::new();
    let bytes = doc.save(&save_options, &mut save_warnings);
    let pdf_bytes = bytes.len();
    fs::write(&output_path, bytes)?;
    let save_elapsed = t4.elapsed();
    eprintln!("[timing] pdf_save: {:.1?} ({} bytes)", save_elapsed, pdf_bytes);

    let total = total_start.elapsed();
    eprintln!("[timing] TOTAL: {:.1?}", total);
    eprintln!("[summary] {} | {} LOC | {} HTML bytes | {} pages | html={:.0?} pdf={:.0?} save={:.0?} total={:.0?}",
        file_name, loc, html.len(), pages,
        html_elapsed, pdf_elapsed, save_elapsed, total);

    println!("Created: {} ({} pages)", output_path.display(), pages);
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
        .arg("--manifest-path")
        .arg(repo_path.join("Cargo.toml"))
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
