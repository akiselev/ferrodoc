//! Inspects a PDF with Ferrodoc's parser boundary for development diagnostics.

use std::{env, error::Error, fs};

use ferrodoc_core::Sha256Digest;
use ferrodoc_pdf::{PdfDocument, PdfLimits};

fn main() -> Result<(), Box<dyn Error>> {
    let path = env::args_os()
        .nth(1)
        .ok_or("usage: cargo run -p ferrodoc-pdf --example inspect -- <file.pdf>")?;
    let document = PdfDocument::from_bytes(fs::read(&path)?, PdfLimits::default())?;
    let inspection = document.inspection();
    println!(
        "digest={} bytes={} objects={} pages={}",
        inspection.digest,
        inspection.bytes.get(),
        document.object_count(),
        inspection.pages.len()
    );
    for page in &inspection.pages {
        let native_chars: usize = page.native_text.iter().map(|span| span.text.len()).sum();
        println!(
            "page={} crop={}x{}pt rotation={} native_chars={}",
            page.index,
            page.crop_box.width(),
            page.crop_box.height(),
            page.rotation,
            native_chars
        );
    }
    if !inspection.pages.is_empty() {
        let raster = document.render_page(0, 96)?;
        println!(
            "first_raster={}x{} rgba_digest={}",
            raster.width,
            raster.height,
            Sha256Digest::of_bytes(&raster.rgba)
        );
    }
    Ok(())
}
