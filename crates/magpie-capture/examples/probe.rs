//! Manual sanity check for the capture backends -- prints what they
//! actually see on this machine right now. Not a test (some of this,
//! like Accessibility trust, depends on machine-specific permission state
//! that can't be asserted in CI), but useful for eyeballing real values
//! during development: `cargo run -p magpie-capture --example probe`.

fn main() {
    #[cfg(target_os = "macos")]
    {
        use magpie_capture::{CaptureBackend, MacosBackend};

        let backend = MacosBackend::new();
        println!("capabilities: {:?}", backend.capabilities());
        println!(
            "accessibility trusted: {}",
            MacosBackend::is_accessibility_trusted()
        );
        println!("frontmost app: {:?}", backend.front_app());
        println!("secure input blocked: {}", backend.secure_input_blocked());
        println!("(current clipboard text: {:?})", backend.read_capture_text());
    }

    #[cfg(not(target_os = "macos"))]
    {
        use magpie_capture::{CaptureBackend, ClipboardBackend};

        let backend = ClipboardBackend::new().expect("clipboard init");
        println!("capabilities: {:?}", backend.capabilities());
        println!("(current clipboard text: {:?})", backend.read_capture_text());
    }
}
