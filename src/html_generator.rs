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
) -> Result<String> {
    let mut html = String::new();
    
    // HTML header with CSS
    html.push_str(&generate_html_header(crate_info, font_size, columns, theme));
    
    // Generate content for each file
    for file in files {
        let file_html = generate_html_for_file(file, syntax_set, theme)?;
        html.push_str(&file_html);
    }
    
    // Close HTML
    html.push_str("</div>\n</body>\n</html>");
    
    Ok(html)
}

/// Generate HTML header with CSS styling
fn generate_html_header(crate_info: &CrateInfo, font_size: f32, columns: u32, theme: Option<&Theme>) -> String {
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

    format!(r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>{name} - Code Review</title>
    <style>
        @page {{
            size: A4;
            margin: 5mm;
        }}
        
        * {{
            box-sizing: border-box;
        }}
        
        body {{
            font-family: 'RobotoMono', 'Fira Code', 'Source Code Pro', 'Consolas', 'Monaco', monospace;
            font-size: {font_size}pt;
            line-height: 1.2;
            margin: 0;
            padding: 0;
            background-color: {bg_color};
            color: {fg_color};
        }}
        
        .content {{
            column-count: {columns};
            column-gap: 10px;
            column-rule: 1px solid #ddd;
            padding: 5px;
        }}
        
        .file-section {{
            break-inside: avoid-column;
            margin-bottom: 10px;
            page-break-inside: avoid;
        }}
        
        .file-header {{
            background-color: #e8e8e8;
            color: #333;
            padding: 3px 8px;
            font-weight: bold;
            font-size: {header_size}pt;
            border-bottom: 1px solid #999;
            margin-bottom: 3px;
            break-after: avoid;
        }}
        
        .code-block {{
            margin: 0;
            padding: 3px;
            overflow-x: hidden;
            white-space: pre-wrap;
            word-wrap: break-word;
            font-size: {font_size}pt;
            line-height: 1.15;
        }}
        
        .line {{
            display: block;
        }}
        
        .line-number {{
            display: inline-block;
            width: 3em;
            text-align: right;
            padding-right: 1em;
            color: #999;
            user-select: none;
            font-size: {line_num_size}pt;
        }}
        
        .line-content {{
            display: inline;
        }}

        h1 {{
            font-size: 16pt;
            margin: 10px 0;
            padding: 10px;
            background-color: #333;
            color: white;
            column-span: all;
        }}

        .crate-info {{
            column-span: all;
            padding: 10px;
            background-color: #f5f5f5;
            margin-bottom: 20px;
            border-left: 4px solid #333;
        }}

        .crate-info h2 {{
            margin: 0 0 5px 0;
            font-size: 14pt;
        }}

        .crate-info p {{
            margin: 5px 0;
            font-size: 10pt;
            color: #666;
        }}
    </style>
</head>
<body>
    <h1>{name} v{version}</h1>
    <div class="crate-info">
        <h2>{name}</h2>
        <p>Version: {version}</p>
        {description}
        <p>Files: {file_count}</p>
    </div>
    <div class="content">
"#,
        name = html_escape(&crate_info.name),
        version = html_escape(&crate_info.version),
        description = crate_info.description.as_ref()
            .map(|d| format!("<p>{}</p>", html_escape(d)))
            .unwrap_or_default(),
        file_count = 0, // Will be updated
        font_size = font_size,
        header_size = font_size + 1.0,
        line_num_size = font_size - 0.5,
        columns = columns,
        bg_color = bg_color,
        fg_color = fg_color,
    )
}

/// Generate HTML for a single source file
fn generate_html_for_file(
    file: &SourceFile,
    syntax_set: &SyntaxSet,
    theme: Option<&Theme>,
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
            
            html.push_str("</span></span>");
        }
    } else {
        // No syntax highlighting - just plain text with line numbers
        for (line_num, line) in LinesWithEndings::from(&content).enumerate() {
            html.push_str(&format!(
                r#"<span class="line"><span class="line-number">{}</span><span class="line-content">{}</span></span>"#,
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
