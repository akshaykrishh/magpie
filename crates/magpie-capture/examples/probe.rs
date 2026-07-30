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
        println!(
            "(current clipboard text: {:?})",
            backend.read_capture_text()
        );

        // Non-interactive whole-screen grab (no -i) so this exercises the
        // Vision OCR pipeline without needing a live selection -- run
        // `cargo run -p magpie-capture --example probe -- ocr` with some
        // text actually visible on screen to eyeball real recognition
        // output.
        if std::env::args().any(|a| a == "ocr") {
            let dir = std::env::temp_dir().join("magpie-probe");
            std::fs::create_dir_all(&dir).unwrap();
            let path = dir.join("ocr-probe.png");
            let status = std::process::Command::new("screencapture")
                .arg(&path)
                .status()
                .expect("screencapture failed to run");
            println!("screencapture exit status: {status}");
            println!("ocr result: {:?}", backend.ocr_image(&path));
        }

        // `cargo run -p magpie-capture --example probe -- ocr-file <path>`
        // re-runs OCR against an existing image instead of grabbing a new
        // one -- used to verify a fix against a real capture that already
        // showed a bug, without needing to reproduce the exact original
        // selection again.
        let args: Vec<String> = std::env::args().collect();
        if let Some(idx) = args.iter().position(|a| a == "ocr-file") {
            if let Some(path) = args.get(idx + 1) {
                println!(
                    "ocr result for {path}: {:?}",
                    backend.ocr_image(std::path::Path::new(path))
                );
            }
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        use magpie_capture::{CaptureBackend, ClipboardBackend};

        let backend = ClipboardBackend::new().expect("clipboard init");
        println!("capabilities: {:?}", backend.capabilities());
        println!(
            "(current clipboard text: {:?})",
            backend.read_capture_text()
        );
    }
}
