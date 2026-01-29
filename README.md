# git2pdf

Convert git repositories to PDF for code review.

## Features

- Clone any GitHub repository via URL or use local paths
- Discover Rust workspace crates automatically
- Classify files as source code vs tests/examples
- Syntax highlighting with customizable themes
- Multi-column layout for efficient space usage
- Configurable paper size and margins
- One PDF per crate

## Installation

```bash
cargo install --path .
```

## Usage

### From a GitHub URL

```bash
git2pdf https://github.com/user/repo
```

### From a local path

```bash
git2pdf /path/to/repository
```

### Options

```
git2pdf - Print git repositories to PDF for code review

Usage: git2pdf [OPTIONS] <SOURCE>

Arguments:
  <SOURCE>  Git repository URL or local file path

Options:
  -r, --ref <REF>             Branch, tag, or commit to checkout
  -o, --output <OUTPUT>       Output directory for generated PDFs [default: .]
      --paper-width <MM>      Paper width in mm [default: 210.0]
      --paper-height <MM>     Paper height in mm [default: 297.0]
      --margin-top <MM>       Top margin in mm [default: 10.0]
      --margin-right <MM>     Right margin in mm [default: 10.0]
      --margin-bottom <MM>    Bottom margin in mm [default: 10.0]
      --margin-left <MM>      Left margin in mm [default: 10.0]
      --font-size <PT>        Font size in points [default: 8.0]
      --columns <N>           Number of columns [default: 2]
      --include-tests         Include test files in output
      --theme <THEME>         Syntax highlighting theme [default: InspiredGitHub]
  -v, --verbose               Verbose output
      --crates <CRATES>       Only process specific crates (comma-separated)
      --temp-dir <PATH>       Temporary directory for cloning
  -h, --help                  Print help
  -V, --version               Print version
```

### Examples

Print a repository with custom margins:

```bash
git2pdf https://github.com/rust-lang/rust --margin-top 15 --margin-bottom 15
```

Print only specific crates:

```bash
git2pdf . --crates "core,utils" --verbose
```

Include tests in the output:

```bash
git2pdf . --include-tests
```

Use a different theme:

```bash
git2pdf . --theme "Solarized (dark)"
```

## Supported Themes

- InspiredGitHub (default)
- Solarized (dark)
- Solarized (light)
- base16-ocean.dark
- base16-eighties.dark
- base16-mocha.dark
- base16-ocean.light

## Output

The tool generates one PDF file per crate, named after the crate (e.g., `my-crate.pdf`).

Each PDF contains:
- A title page with crate information
- All source files with syntax highlighting
- Line numbers for easy reference
- Multi-column layout for efficient space usage

## Requirements

- Rust 1.70 or later
- Git (for cloning repositories)

## License

MIT
