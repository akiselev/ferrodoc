#![no_main]

use ferrodoc_core::ModelManifest;
use ferrodoc_ir::Document;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = serde_json::from_slice::<ModelManifest>(data);
    if let Ok(document) = serde_json::from_slice::<Document>(data) {
        let _ = document.validate();
    }
});
