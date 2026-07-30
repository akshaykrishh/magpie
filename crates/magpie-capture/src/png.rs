use std::path::Path;

/// Reads the width/height out of a PNG's IHDR chunk directly rather than
/// pulling in an image-parsing crate for two integers: the 8-byte PNG
/// signature is always immediately followed by the IHDR chunk, whose width
/// and height are big-endian u32s at fixed offsets 16 and 20 -- part of the
/// PNG spec itself, not something specific to any one platform's screenshot
/// tool that could drift.
pub(crate) fn png_dimensions(path: &Path) -> Option<(u32, u32)> {
    let bytes = std::fs::read(path).ok()?;
    if bytes.len() < 24 || bytes[0..8] != [0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n'] {
        return None;
    }
    let width = u32::from_be_bytes(bytes[16..20].try_into().ok()?);
    let height = u32::from_be_bytes(bytes[20..24].try_into().ok()?);
    Some((width, height))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_ihdr_chunk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.png");
        let mut bytes = vec![0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n'];
        bytes.extend_from_slice(b"\0\0\0\rIHDR"); // chunk length + type, unused by the parser
        bytes.extend_from_slice(&800u32.to_be_bytes());
        bytes.extend_from_slice(&600u32.to_be_bytes());
        std::fs::write(&path, &bytes).unwrap();

        assert_eq!(png_dimensions(&path), Some((800, 600)));
    }

    #[test]
    fn rejects_a_file_that_is_not_a_png() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("not-a-png.txt");
        std::fs::write(&path, b"just some text, not an image").unwrap();

        assert_eq!(png_dimensions(&path), None);
    }
}
