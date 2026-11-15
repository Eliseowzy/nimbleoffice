#![no_main]

use libfuzzer_sys::fuzz_target;
use nimbleoffice_xlsx::fuzzing::recovery_decode;

fuzz_target!(|data: &[u8]| {
    recovery_decode(data);
});
