use std::path::{Path, PathBuf};

use ashpd::desktop::screenshot::{Screenshot, ScreenshotProxy};
use ashpd::desktop::ResponseError;

use crate::backend::{Capabilities, CaptureBackend, CaptureMode, ScreenshotCapture, SourceInfo};
use crate::clipboard::ClipboardBackend;
use crate::error::{Error, Result};

/// NOTE: unlike the macOS backend, none of the portal/OCR paths in this file
/// run against a real Linux desktop in CI or anywhere else in this
/// repository's toolchain -- there is no Linux machine to test against.
/// Region capture and OCR are implemented to match the freedesktop portal
/// specification and the `ashpd`/`tesseract` documentation, and the whole
/// crate type-checks against the `aarch64-unknown-linux-gnu` target, but
/// that is compile-time verification only, not a substitute for running it.
/// Treat this file as needing real-hardware verification before being
/// trusted.
///
/// Text capture (clipboard) is unaffected by any of that -- it's the same
/// `ClipboardBackend` every other non-macOS target uses, which the rest of
/// this crate's test suite does exercise.
pub struct LinuxBackend {
    clipboard: ClipboardBackend,
    /// Probed once at construction, not on every `capabilities()` call --
    /// a D-Bus round trip on every capture would add needless latency to
    /// the hot path. Whether a portal implementing the Screenshot interface
    /// is reachable can change while magpie is running (a user could start
    /// a portal-providing session mid-run), but this crate's own precedent
    /// (`FreshnessTracker`, seeded once at construction) already accepts
    /// that trade-off for infrequently-changing environment state.
    screenshot_available: bool,
    tesseract_available: bool,
}

impl LinuxBackend {
    pub fn new() -> Result<Self> {
        let clipboard = ClipboardBackend::new()?;
        let screenshot_available =
            async_io::block_on(async { ScreenshotProxy::new().await.is_ok() });
        let tesseract_available = std::process::Command::new("tesseract")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        Ok(Self {
            clipboard,
            screenshot_available,
            tesseract_available,
        })
    }
}

impl CaptureBackend for LinuxBackend {
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            mode: CaptureMode::ClipboardOnly,
            synthesized_copy_available: false,
            screenshot_available: self.screenshot_available,
            ocr_available: self.tesseract_available,
        }
    }

    fn read_capture_text(&self) -> Result<Option<String>> {
        self.clipboard.read_capture_text()
    }

    fn front_app(&self) -> Result<Option<SourceInfo>> {
        self.clipboard.front_app()
    }

    /// Interactive region/window capture via the `org.freedesktop.portal.
    /// Screenshot` portal (`interactive: true`), which hands the actual
    /// selection UI off to whatever the running desktop environment
    /// provides (GNOME's own picker, KDE's, wlroots' slurp-based one under
    /// xdg-desktop-portal-wlr) -- this is the standardized, sanctioned way
    /// to do this on both X11 and Wayland sessions, not a Wayland-only
    /// workaround. Deliberately not a hand-rolled X11 selection overlay:
    /// that would need real hardware to get right and this doesn't.
    fn capture_screenshot_region(&self, dest_dir: &Path) -> Result<Option<ScreenshotCapture>> {
        if !self.screenshot_available {
            return Ok(None);
        }
        std::fs::create_dir_all(dest_dir)?;

        let sent = async_io::block_on(async {
            Screenshot::request()
                .interactive(true)
                .modal(true)
                .send()
                .await
        });

        let screenshot = match sent.and_then(|request| request.response()) {
            Ok(s) => s,
            // The user pressing Escape/Cancel in the portal's own dialog --
            // a normal outcome, matching `read_capture_text`'s `Ok(None)`
            // for "nothing to capture", not a failure.
            Err(ashpd::Error::Response(ResponseError::Cancelled)) => return Ok(None),
            Err(e) => return Err(Error::Portal(e.to_string())),
        };

        let src_path = file_uri_to_path(screenshot.uri().as_str()).ok_or_else(|| {
            Error::Portal(format!(
                "portal returned a non-local screenshot URI: {}",
                screenshot.uri()
            ))
        })?;

        // The portal spec guarantees PNG output, but not that the file
        // lives anywhere near magpie's own blobs directory (usually
        // ~/Pictures/Screenshots or a portal-managed cache dir) -- moving
        // it in is what makes the blob's stored path stable regardless of
        // where the portal happens to write, and rename can fail across
        // filesystems (EXDEV), which copy-then-remove doesn't.
        let millis = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        let dest_path = dest_dir.join(format!("shot-{millis}.png"));
        if std::fs::rename(&src_path, &dest_path).is_err() {
            std::fs::copy(&src_path, &dest_path)?;
            let _ = std::fs::remove_file(&src_path);
        }

        let (width, height) = crate::png::png_dimensions(&dest_path).unwrap_or((0, 0));
        Ok(Some(ScreenshotCapture {
            path: dest_path,
            mime: "image/png".to_string(),
            width,
            height,
        }))
    }

    /// Shells out to `tesseract` if it's present on PATH -- checked once at
    /// construction (`capabilities().ocr_available`), never assumed. Unlike
    /// macOS's Vision framework, there's no OCR engine built into every
    /// Linux desktop, so this degrades to "not available" rather than
    /// failing when it isn't installed, matching how every other permission
    /// or capability gap in this crate degrades.
    fn ocr_image(&self, path: &Path) -> Result<Option<String>> {
        if !self.tesseract_available {
            return Ok(None);
        }
        let output = std::process::Command::new("tesseract")
            .arg(path)
            .arg("stdout")
            .output()?;
        if !output.status.success() {
            return Err(Error::Ocr(
                String::from_utf8_lossy(&output.stderr).into_owned(),
            ));
        }
        let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Ok(if text.is_empty() { None } else { Some(text) })
    }
}

/// `file:///path/with%20spaces.png` -> `/path/with spaces.png`. The portal
/// spec only promises a `file://` URI for local screenshots (there's no
/// other kind it would return in practice, since the compositor always
/// writes the capture to local disk before responding), so a non-file
/// scheme is treated as an error by the caller rather than silently
/// ignored.
fn file_uri_to_path(uri: &str) -> Option<PathBuf> {
    let encoded_path = uri.strip_prefix("file://")?;
    let decoded = percent_encoding::percent_decode_str(encoded_path)
        .decode_utf8()
        .ok()?;
    Some(PathBuf::from(decoded.into_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_uri_to_path_decodes_percent_escapes() {
        assert_eq!(
            file_uri_to_path("file:///home/user/Pictures/Screenshot%20from%202026.png"),
            Some(PathBuf::from(
                "/home/user/Pictures/Screenshot from 2026.png"
            ))
        );
    }

    #[test]
    fn file_uri_to_path_rejects_non_file_schemes() {
        assert_eq!(file_uri_to_path("http://example.com/shot.png"), None);
    }
}
