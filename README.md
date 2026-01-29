# git2pdf

Convert git repositories to PDF for code review.

## Features

- Clone any GitHub repository via URL or use local paths
- Automatic `cargo fmt` on cloned repositories (configurable line width)
- Discover Rust workspace crates automatically
- Classify files as source code vs tests/examples
- Syntax highlighting with customizable themes (or disable with `--theme none`)
- Multi-column layout for efficient space usage
- Configurable paper size, margins, and font
- Custom TTF font support (embedded RobotoMono-Bold by default)
- Respects `.gitignore` when copying files
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
      --paper-size <WxH>      Paper size as WIDTHxHEIGHT in mm [default: 210x297]
      --margins <MARGINS>     Margins in mm, CSS-style: "all", "v h", or "t r b l" [default: 5]
      --font-size <PT>        Font size in points [default: 6.0]
      --columns <N>           Number of columns [default: 2]
      --include-tests         Include test files in output
      --theme <THEME>         Syntax highlighting theme, or "none" to disable [default: InspiredGitHub]
      --no-fmt                Skip running cargo fmt
      --line-width <N>        Line width for rustfmt [default: 80]
      --font <PATH>           Path to a TTF font file (default: embedded RobotoMono-Bold)
  -v, --verbose               Verbose output
      --crates <CRATES>       Only process specific crates (comma-separated)
      --temp-dir <PATH>       Temporary directory for cloning
  -h, --help                  Print help
  -V, --version               Print version
```

### Examples

Print a repository with custom paper size and margins:

```bash
git2pdf https://github.com/rust-lang/rust --paper-size 200x280 --margins "15 20"
```

Print only specific crates:

```bash
git2pdf . --crates "core,utils" --verbose
```

Include tests in the output:

```bash
git2pdf . --include-tests
```

Use a different theme or disable syntax highlighting:

```bash
git2pdf . --theme "Solarized (dark)"
git2pdf . --theme none
```

Skip cargo fmt or use custom line width:

```bash
git2pdf . --no-fmt
git2pdf https://github.com/user/repo --line-width 100
```

Use a custom font:

```bash
git2pdf . --font /path/to/MyFont.ttf
```

## Supported Themes

- `InspiredGitHub` (default)
- `Solarized (dark)`
- `Solarized (light)`
- `base16-ocean.dark`
- `base16-eighties.dark`
- `base16-mocha.dark`
- `base16-ocean.light`
- `none` (disables syntax highlighting)

## Requirements

- Rust 1.70 or later

## License

MIT
