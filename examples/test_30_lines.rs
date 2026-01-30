//! Test for 30 lines with IFC debugging
use std::collections::BTreeMap;
use std::fs;
use printpdf::{Mm, PageMargins, PdfDocument, PdfSaveOptions, XmlRenderOptions};

fn main() -> anyhow::Result<()> {
    let html = fs::read_to_string("tests/test_30_lines.html")?;
    
    println!("HTML length: {} bytes", html.len());
    
    // Load RobotoMono font
    let font_bytes = fs::read("fonts/RobotoMono-Bold.ttf")?;
    let mut fonts = BTreeMap::new();
    fonts.insert("RobotoMono".to_string(), font_bytes);
    
    let options = XmlRenderOptions {
        page_width: Mm(210.0),  // A4
        page_height: Mm(297.0),
        margins: PageMargins {
            top: Mm(10.0),
            right: Mm(10.0),
            bottom: Mm(10.0),
            left: Mm(10.0),
        },
        fonts,
        ..Default::default()
    };
    
    // Use debug API to get detailed output
    let (pages, font_data, debug_info) = printpdf::xml_to_pdf_pages_debug(&html, &options)
        .map_err(|warnings| {
            for w in &warnings {
                eprintln!("Warning: {:?}", w);
            }
            anyhow::anyhow!("PDF generation failed with {} warnings", warnings.len())
        })?;
    
    println!("\nGenerated {} pages", pages.len());
    println!("Font data: {} fonts", font_data.len());
    
    // Save all debug info
    for (i, debug_str) in debug_info.display_list_debug.iter().enumerate() {
        let filename = format!("tests/test_30_lines_debug_{}.txt", i);
        fs::write(&filename, debug_str)?;
        println!("Saved debug info to {}", filename);
    }
    
    // Create PDF document
    let mut binding = PdfDocument::new("Test 30 Lines");
    
    // IMPORTANT: Register fonts from the layout into the PDF document
    for (font_hash, parsed_font) in font_data.iter() {
        let font_id = printpdf::FontId(format!("F{}", font_hash.font_hash));
        let pdf_font = printpdf::PdfFont::new(parsed_font.clone());
        binding.resources.fonts.map.insert(font_id, pdf_font);
    }
    
    let doc = binding.with_pages(pages);
    
    // Save
    let save_opts = PdfSaveOptions::default();
    let mut save_warnings = Vec::new();
    let bytes = doc.save(&save_opts, &mut save_warnings);
    
    fs::write("tests/test_30_lines.pdf", bytes)?;
    println!("\nSaved to tests/test_30_lines.pdf");
    
    Ok(())
}
