//! What a picture's own header says about it that decoding it does not.

// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

/// The resolution a picture was saved at, across and down, in dots per inch:
/// a PNG's `pHYs` chunk or a JPEG's JFIF density. `None` where the picture
/// does not say, which `LibreOffice` reads as 96.
pub fn dots_per_inch(bytes: &[u8]) -> Option<(f32, f32)> {
    match bytes {
        [0x89, b'P', b'N', b'G', ..] => png(bytes),
        [0xFF, 0xD8, ..] => jpeg(bytes),
        _ => None,
    }
}

fn png(bytes: &[u8]) -> Option<(f32, f32)> {
    let mut at = 8;
    while let Some(header) = bytes.get(at..at + 8) {
        let length = usize::try_from(u32::from_be_bytes(header[..4].try_into().ok()?)).ok()?;
        let data = bytes.get(at + 8..at + 8 + length)?;
        match &header[4..] {
            b"pHYs" if length == 9 && data[8] == 1 => {
                let per_metre = |at: usize| {
                    #[allow(clippy::cast_precision_loss)]
                    let n = u32::from_be_bytes(data[at..at + 4].try_into().ok()?) as f32;
                    (n > 0.0).then_some(n * 0.0254)
                };
                return Some((per_metre(0)?, per_metre(4)?));
            }
            b"IDAT" | b"IEND" => return None,
            _ => {}
        }
        at += 12 + length;
    }
    None
}

fn jpeg(bytes: &[u8]) -> Option<(f32, f32)> {
    let mut at = 2;
    while let [0xFF, marker, high, low, ..] = *bytes.get(at..)? {
        let length = usize::from(u16::from_be_bytes([high, low]));
        let data = bytes.get(at + 4..at + 2 + length)?;
        match marker {
            0xE0 if data.starts_with(b"JFIF\0") && data.len() >= 12 => {
                let density = |at: usize| f32::from(u16::from_be_bytes([data[at], data[at + 1]]));
                let per_inch = match data[7] {
                    1 => 1.0,
                    2 => 2.54,
                    _ => return None,
                };
                let (across, down) = (density(8) * per_inch, density(10) * per_inch);
                return (across > 0.0 && down > 0.0).then_some((across, down));
            }
            0xDA | 0xD9 => return None,
            _ => {}
        }
        at += 2 + length;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::dots_per_inch;

    fn png(chunks: &[(&[u8; 4], &[u8])]) -> Vec<u8> {
        let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        for (kind, data) in chunks {
            bytes.extend(u32::try_from(data.len()).expect("small").to_be_bytes());
            bytes.extend(*kind);
            bytes.extend(*data);
            bytes.extend([0; 4]);
        }
        bytes
    }

    #[test]
    fn a_png_says_its_resolution_in_dots_per_metre() {
        let mut physical = Vec::new();
        physical.extend(11_811u32.to_be_bytes());
        physical.extend(11_811u32.to_be_bytes());
        physical.push(1);
        let bytes = png(&[(b"IHDR", &[0; 13]), (b"pHYs", &physical), (b"IDAT", &[])]);
        let (across, down) = dots_per_inch(&bytes).expect("a resolution");
        assert!((across - 300.0).abs() < 0.01 && (down - 300.0).abs() < 0.01);
    }

    #[test]
    fn a_png_that_says_nothing_has_no_resolution() {
        assert_eq!(
            dots_per_inch(&png(&[(b"IHDR", &[0; 13]), (b"IDAT", &[])])),
            None
        );
    }

    #[test]
    fn a_jpeg_says_its_resolution_in_its_jfif_header() {
        let mut bytes = vec![0xFF, 0xD8, 0xFF, 0xE0, 0, 16];
        bytes.extend(b"JFIF\0");
        bytes.extend([1, 1, 1, 0, 72, 0, 72, 0, 0]);
        bytes.extend([0xFF, 0xDA, 0, 2]);
        assert_eq!(dots_per_inch(&bytes), Some((72.0, 72.0)));
    }
}
