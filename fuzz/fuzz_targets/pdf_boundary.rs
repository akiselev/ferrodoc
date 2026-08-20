#![no_main]

use ferrodoc_pdf::{PdfDocument, PdfLimits};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() <= 4 * 1024 * 1024 {
        let _ = PdfDocument::from_bytes(data.to_vec(), PdfLimits::default());
    }
});
