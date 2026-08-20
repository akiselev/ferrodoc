#![no_main]

use ferrodoc_ir::Document;
use ferrodoc_render::{OutputFormat, render};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(document) = serde_json::from_slice::<Document>(data)
        && document.validate().is_ok()
    {
        let _ = render(&document, OutputFormat::Markdown);
        let _ = render(&document, OutputFormat::Html);
        let _ = render(&document, OutputFormat::EvidenceJson);
    }
});
