#![no_main]

use std::io::Cursor;

use ferrodoc_protocol::{HostMessage, MAX_FRAME_LENGTH, read_frame, read_preamble};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let mut cursor = Cursor::new(data);
    let _ = read_preamble(&mut cursor);
    let _ = read_frame::<HostMessage>(&mut cursor, MAX_FRAME_LENGTH);
});
