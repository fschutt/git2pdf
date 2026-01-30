//! HTML generation with syntax highlighting
//!
//! Generates HTML from source files using syntect for syntax highlighting.

use std::fs;

use anyhow::{Context, Result};
use syntect::highlighting::{Theme, Style, FontStyle};
use syntect::parsing::SyntaxSet;
use syntect::easy::HighlightLines;
use syntect::util::LinesWithEndings;

use crate::crate_discovery::CrateInfo;
use crate::file_classifier::SourceFile;

/// Generate HTML for an entire crate
pub fn generate_html_for_crate(
    crate_info: &CrateInfo,
    files: &[&SourceFile],
    syntax_set: &SyntaxSet,
    theme: Option<&Theme>,
    font_size: f32,
    columns: u32,
    page_break: bool,
) -> Result<String> {
    let mut html = String::new();
    
    // HTML header with CSS
    html.push_str(&generate_html_header(crate_info, font_size, columns, theme, page_break));
    
    // Open the content div that has column-count applied
    if columns > 1 {
        html.push_str("<div class=\"content\">\n");
    }
    
    // Generate content for each file
    for file in files {
        let file_html = generate_html_for_file(file, syntax_set, theme, font_size)?;
        html.push_str(&file_html);
    }
    
    // Close content div if using columns
    if columns > 1 {
        html.push_str("</div>\n");
    }
    
    // Close HTML
    html.push_str("</body>\n</html>");
    
    Ok(html)
}

/// Generate HTML header with CSS styling
fn generate_html_header(crate_info: &CrateInfo, font_size: f32, columns: u32, theme: Option<&Theme>, page_break: bool) -> String {
    let (bg_color, fg_color) = if let Some(t) = theme {
        let bg = t.settings.background
            .map(|c| format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b))
            .unwrap_or_else(|| "#ffffff".to_string());
        let fg = t.settings.foreground
            .map(|c| format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b))
            .unwrap_or_else(|| "#000000".to_string());
        (bg, fg)
    } else {
        ("#ffffff".to_string(), "#000000".to_string())
    };
    
    let page_break_css = if page_break {
        "page-break-after: always;"
    } else {
        ""
    };
    
    // Column CSS only if columns > 1
    let column_css = if columns > 1 {
        format!(r#"
        .content {{
            column-count: {columns};
            column-gap: 10px;
            column-rule: 1px solid #ddd;
        }}"#, columns = columns)
    } else {
        String::new()
    };

    format!(r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <title>{name} - Code Review</title>
    <style>
        * {{
            box-sizing: border-box;
            margin: 0;
            padding: 0;
        }}
        
        body {{
            font-family: 'RobotoMono', monospace;
            font-size: {font_size}pt;
            line-height: 1.2;
            background-color: {bg_color};
            color: {fg_color};
        }}
        {column_css}
        .file-section {{
            margin-bottom: 5px;
            {page_break_css}
        }}
        
        .file-header {{
            background-color: #e0e0e0;
            color: #333;
            padding: 2px 5px;
            font-weight: bold;
            font-size: {header_size}pt;
            border-bottom: 1px solid #999;
        }}
        
        .code-block {{
            white-space: pre-wrap;
            word-wrap: break-word;
            font-size: {font_size}pt;
            font-family: 'RobotoMono', monospace;
            line-height: 1.15;
        }}
        
        .line {{
            display: block;
        }}
        
        .line-number {{
            display: inline-block;
            width: 2.5em;
            text-align: right;
            padding-right: 0.5em;
            color: #888;
            font-size: {line_num_size}pt;
        }}
        
        .line-content {{
            display: inline;
        }}

        h1 {{
            font-size: 18pt;
            padding: 8px;
            background-color: #333;
            color: white;
        }}

        .crate-info {{
            padding: 8px;
            background-color: #f0f0f0;
            margin-bottom: 10px;
            border-left: 3px solid #333;
        }}

        .crate-info p {{
            margin: 2px 0;
            font-size: 9pt;
            color: #555;
        }}
    </style>
</head>
<body>
    <h1>{name} v{version}</h1>
    <div class="crate-info">
        <p><b>{name}</b> - Version {version}</p>
        {description}
    </div>
"#,
        name = html_escape(&crate_info.name),
        version = html_escape(&crate_info.version),
        description = crate_info.description.as_ref()
            .map(|d| format!("<p>{}</p>", html_escape(d)))
            .unwrap_or_default(),
        font_size = font_size,
        header_size = font_size + 1.0,
        line_num_size = font_size,
        column_css = column_css,
        page_break_css = page_break_css,
        bg_color = bg_color,
        fg_color = fg_color,
    )
}

/// Generate HTML for a single source file
fn generate_html_for_file(
    file: &SourceFile,
    syntax_set: &SyntaxSet,
    theme: Option<&Theme>,
    _font_size: f32,
) -> Result<String> {
    let content = fs::read_to_string(&file.path)
        .with_context(|| format!("Failed to read file: {}", file.path.display()))?;
    
    let mut html = String::new();
    
    // File section - show relative path from crate root
    html.push_str(&format!(
        r#"<div class="file-section">
<div class="file-header">{}</div>
<pre class="code-block">"#,
        html_escape(&file.relative_path.to_string_lossy()),
    ));
    
    // Highlight each line (or just escape if no theme)
    if let Some(theme) = theme {
        // Get syntax for Rust
        let syntax = syntax_set.find_syntax_by_extension("rs")
            .unwrap_or_else(|| syntax_set.find_syntax_plain_text());
        
        let mut highlighter = HighlightLines::new(syntax, theme);
        
        for (line_num, line) in LinesWithEndings::from(&content).enumerate() {
            let highlighted = highlighter.highlight_line(line, syntax_set)
                .unwrap_or_else(|_| vec![(Style::default(), line)]);
            
            html.push_str(&format!(
                r#"<span class="line"><span class="line-number">{}</span><span class="line-content">"#,
                line_num + 1
            ));
            
            for (style, text) in highlighted {
                let css = style_to_css(&style);
                if css.is_empty() {
                    html.push_str(&html_escape(text));
                } else {
                    html.push_str(&format!(
                        r#"<span style="{}">{}</span>"#,
                        css,
                        html_escape(text)
                    ));
                }
            }
            
            // Close the line content and line spans, then add a newline
            // The newline is important for the PDF layout engine to recognize line breaks
            html.push_str("</span></span>\n");
        }
    } else {
        // No syntax highlighting - just plain text with line numbers
        for (line_num, line) in LinesWithEndings::from(&content).enumerate() {
            html.push_str(&format!(
                r#"<span class="line"><span class="line-number">{}</span><span class="line-content">{}</span></span>
"#,
                line_num + 1,
                html_escape(line)
            ));
        }
    }
    
    html.push_str("</pre>\n</div>\n");
    
    Ok(html)
}

/// Convert a syntect Style to CSS
fn style_to_css(style: &Style) -> String {
    let mut css_parts = Vec::new();
    
    // Foreground color
    let fg = style.foreground;
    if fg.a > 0 {
        css_parts.push(format!("color: #{:02x}{:02x}{:02x}", fg.r, fg.g, fg.b));
    }
    
    // Font style
    if style.font_style.contains(FontStyle::BOLD) {
        css_parts.push("font-weight: bold".to_string());
    }
    if style.font_style.contains(FontStyle::ITALIC) {
        css_parts.push("font-style: italic".to_string());
    }
    if style.font_style.contains(FontStyle::UNDERLINE) {
        css_parts.push("text-decoration: underline".to_string());
    }
    
    css_parts.join("; ")
}

/// Escape HTML special characters
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// Generate a minimal HTML document for a single file (no headers, for parallel processing)
pub fn generate_html_for_single_file(
    file: &SourceFile,
    syntax_set: &SyntaxSet,
    theme: Option<&Theme>,
    font_size: f32,
) -> Result<String> {
    let content = fs::read_to_string(&file.path)
        .with_context(|| format!("Failed to read file: {}", file.path.display()))?;
    
    let (bg_color, fg_color) = if let Some(t) = theme {
        let bg = t.settings.background
            .map(|c| format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b))
            .unwrap_or_else(|| "#ffffff".to_string());
        let fg = t.settings.foreground
            .map(|c| format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b))
            .unwrap_or_else(|| "#000000".to_string());
        (bg, fg)
    } else {
        ("#ffffff".to_string(), "#000000".to_string())
    };

    let mut html = format!(r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <title>{path}</title>
    <style>
        * {{
            box-sizing: border-box;
            margin: 0;
            padding: 0;
        }}
        
        body {{
            font-family: 'RobotoMono', monospace;
            font-size: {font_size}pt;
            line-height: 1.2;
            background-color: {bg_color};
            color: {fg_color};
        }}
        
        .file-header {{
            background-color: #e0e0e0;
            color: #333;
            padding: 2px 5px;
            font-weight: bold;
            font-size: {header_size}pt;
            border-bottom: 1px solid #999;
        }}
        
        .code-block {{
            white-space: pre-wrap;
            word-wrap: break-word;
            font-size: {font_size}pt;
            font-family: 'RobotoMono', monospace;
            line-height: 1.15;
        }}
        
        .line {{
            display: block;
        }}
        
        .line-number {{
            display: inline-block;
            width: 2.5em;
            text-align: right;
            padding-right: 0.5em;
            color: #888;
            font-size: {line_num_size}pt;
        }}
        
        .line-content {{
            display: inline;
        }}
    </style>
</head>
<body>
<div class="file-header">{path}</div>
<pre class="code-block">"#,
        path = html_escape(&file.relative_path.to_string_lossy()),
        font_size = font_size,
        header_size = font_size + 1.0,
        line_num_size = font_size,
        bg_color = bg_color,
        fg_color = fg_color,
    );
    
    // Highlight each line (or just escape if no theme)
    if let Some(theme) = theme {
        let syntax = syntax_set.find_syntax_by_extension("rs")
            .unwrap_or_else(|| syntax_set.find_syntax_plain_text());
        
        let mut highlighter = HighlightLines::new(syntax, theme);
        
        for (line_num, line) in LinesWithEndings::from(&content).enumerate() {
            let highlighted = highlighter.highlight_line(line, syntax_set)
                .unwrap_or_else(|_| vec![(Style::default(), line)]);
            
            html.push_str(&format!(
                r#"<span class="line"><span class="line-number">{}</span><span class="line-content">"#,
                line_num + 1
            ));
            
            for (style, text) in highlighted {
                let css = style_to_css(&style);
                if css.is_empty() {
                    html.push_str(&html_escape(text));
                } else {
                    html.push_str(&format!(
                        r#"<span style="{}">{}</span>"#,
                        css,
                        html_escape(text)
                    ));
                }
            }
            
            html.push_str("</span></span>\n");
        }
    } else {
        for (line_num, line) in LinesWithEndings::from(&content).enumerate() {
            html.push_str(&format!(
                r#"<span class="line"><span class="line-number">{}</span><span class="line-content">{}</span></span>
"#,
                line_num + 1,
                html_escape(line)
            ));
        }
    }
    
    html.push_str("</pre>\n</body>\n</html>");
    
    Ok(html)
}

/// Generate a title page HTML for a crate
pub fn generate_title_page_html(
    crate_info: &CrateInfo,
    git_hash: Option<&str>,
    font_size: f32,
) -> String {
    format!(r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <title>{name} - Title</title>
    <style>
        * {{
            box-sizing: border-box;
            margin: 0;
            padding: 0;
        }}
        
        body {{
            font-family: 'RobotoMono', monospace;
            font-size: {font_size}pt;
            display: flex;
            flex-direction: column;
            justify-content: center;
            align-items: center;
            min-height: 100vh;
            background-color: #ffffff;
            color: #333;
        }}
        
        .title-container {{
            text-align: center;
            padding: 40px;
        }}
        
        h1 {{
            font-size: 36pt;
            margin-bottom: 20px;
            color: #222;
        }}
        
        .version {{
            font-size: 18pt;
            color: #666;
            margin-bottom: 10px;
        }}
        
        .git-hash {{
            font-size: 12pt;
            color: #888;
            font-family: 'RobotoMono', monospace;
            margin-bottom: 20px;
        }}
        
        .description {{
            font-size: 14pt;
            color: #555;
            max-width: 600px;
            line-height: 1.5;
        }}
    </style>
</head>
<body>
    <div class="title-container">
        <h1>{name}</h1>
        <div class="version">Version {version}</div>
        {git_hash_html}
        {description_html}
    </div>
</body>
</html>"#,
        name = html_escape(&crate_info.name),
        version = html_escape(&crate_info.version),
        git_hash_html = git_hash
            .map(|h| format!(r#"<div class="git-hash">Commit: {}</div>"#, html_escape(h)))
            .unwrap_or_default(),
        description_html = crate_info.description.as_ref()
            .map(|d| format!(r#"<div class="description">{}</div>"#, html_escape(d)))
            .unwrap_or_default(),
        font_size = font_size,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_html_escape() {
        assert_eq!(html_escape("<div>"), "&lt;div&gt;");
        assert_eq!(html_escape("a & b"), "a &amp; b");
        assert_eq!(html_escape("\"test\""), "&quot;test&quot;");
    }
}
