//! Drag-time classification: decide what a clipboard entry *is* and build
//! the payload a drop target can consume. Pure Rust, no GTK types — the
//! GdkContentProvider is a thin byte-backed adapter over [`Payload`] built
//! elsewhere, which keeps this whole module testable without a display.

use std::path::{Path, PathBuf};

pub const URI_LIST: &str = "text/uri-list";
pub const TEXT_PLAIN: &str = "text/plain;charset=utf-8";
pub const OCTET_STREAM: &str = "application/octet-stream";

/// What an entry turned out to be. Drives the puck label, not the
/// offered formats — those live in [`Payload::formats`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    File,
    Files(usize),
    Image(&'static str),
    Binary,
    Text,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Payload {
    pub kind: Kind,
    /// (MIME type, bytes) pairs, preferred format first. Offered to the
    /// receiver as a union; the receiver picks.
    pub formats: Vec<(String, Vec<u8>)>,
}

pub fn classify(content: &[u8]) -> Payload {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    classify_with_home(content, home.as_deref())
}

/// `home` backs `~` expansion; injectable so tests control the filesystem.
pub fn classify_with_home(content: &[u8], home: Option<&Path>) -> Payload {
    if let Some(mime) = sniff_image(content) {
        return Payload {
            kind: Kind::Image(mime),
            formats: vec![(mime.to_string(), content.to_vec())],
        };
    }

    if looks_binary(content) {
        return Payload {
            kind: Kind::Binary,
            formats: vec![(OCTET_STREAM.to_string(), content.to_vec())],
        };
    }
    let text = std::str::from_utf8(content).expect("checked by looks_binary");

    let lines: Vec<&str> = text.trim().lines().map(str::trim).collect();
    let paths: Option<Vec<PathBuf>> = lines
        .iter()
        .map(|line| existing_absolute_path(line, home))
        .collect();

    match paths {
        Some(paths) if !paths.is_empty() => {
            let uris: String = paths.iter().map(|p| file_uri(p)).collect();
            let kind = match paths.len() {
                1 => Kind::File,
                n => Kind::Files(n),
            };
            Payload {
                kind,
                formats: vec![
                    (URI_LIST.to_string(), uris.into_bytes()),
                    (TEXT_PLAIN.to_string(), text.trim().as_bytes().to_vec()),
                ],
            }
        }
        _ => Payload {
            kind: Kind::Text,
            formats: vec![(TEXT_PLAIN.to_string(), content.to_vec())],
        },
    }
}

/// A line counts as a path only if, after expanding a leading `~`, it is
/// absolute and exists (file or directory). Relative paths never qualify:
/// spawned from a keybind the cwd is meaningless, and a copied word like
/// `Documents` must not silently become a file drop.
fn existing_absolute_path(line: &str, home: Option<&Path>) -> Option<PathBuf> {
    if line.is_empty() {
        return None;
    }
    let expanded: PathBuf = if line == "~" {
        home?.to_path_buf()
    } else if let Some(rest) = line.strip_prefix("~/") {
        home?.join(rest)
    } else {
        PathBuf::from(line)
    };
    (expanded.is_absolute() && expanded.exists()).then_some(expanded)
}

/// RFC 8089 file URI with RFC 3986 percent-encoding, one per line,
/// CRLF-terminated as text/uri-list requires.
fn file_uri(path: &Path) -> String {
    let mut out = String::from("file://");
    for &b in path.as_os_str().as_encoded_bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~' | b'/') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out.push_str("\r\n");
    out
}

/// Not text: invalid UTF-8, or a NUL byte, which GDK's C-string text
/// path would truncate at (and a debug build asserts on).
pub fn looks_binary(content: &[u8]) -> bool {
    content.contains(&0) || std::str::from_utf8(content).is_err()
}

/// [`looks_binary`] for the first bytes of an entry: a cut mid-character
/// is still text, so list previews agree with drag-time classification
/// on everything both can see.
pub fn prefix_looks_binary(prefix: &[u8]) -> bool {
    prefix.contains(&0) || std::str::from_utf8(prefix).is_err_and(|e| e.error_len().is_some())
}

pub fn human_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{} KiB", bytes / 1024)
    } else {
        format!("{:.1} MiB", bytes as f64 / (1024.0 * 1024.0))
    }
}

/// Pixel dimensions from an image header. Works on the same short
/// prefix previews are built from: PNG, GIF and WebP keep them at fixed
/// offsets, JPEG behind variable-length segments (EXIF can push the
/// frame header past any prefix), so a large JPEG header yields `None`
/// rather than more I/O.
pub fn image_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    match sniff_image(bytes)? {
        "image/png" => png_dimensions(bytes),
        "image/gif" => gif_dimensions(bytes),
        "image/webp" => webp_dimensions(bytes),
        "image/jpeg" => jpeg_dimensions(bytes),
        _ => None,
    }
}

fn be16(b: &[u8]) -> Option<u32> {
    Some(u16::from_be_bytes(b.get(..2)?.try_into().ok()?) as u32)
}

fn le16(b: &[u8]) -> Option<u32> {
    Some(u16::from_le_bytes(b.get(..2)?.try_into().ok()?) as u32)
}

fn be32(b: &[u8]) -> Option<u32> {
    Some(u32::from_be_bytes(b.get(..4)?.try_into().ok()?))
}

fn le24(b: &[u8]) -> Option<u32> {
    let b = b.get(..3)?;
    Some(b[0] as u32 | (b[1] as u32) << 8 | (b[2] as u32) << 16)
}

fn png_dimensions(b: &[u8]) -> Option<(u32, u32)> {
    // IHDR is required to be the first chunk
    (b.get(12..16)? == b"IHDR").then_some(())?;
    Some((be32(b.get(16..)?)?, be32(b.get(20..)?)?))
}

fn gif_dimensions(b: &[u8]) -> Option<(u32, u32)> {
    Some((le16(b.get(6..)?)?, le16(b.get(8..)?)?))
}

fn webp_dimensions(b: &[u8]) -> Option<(u32, u32)> {
    let payload = b.get(20..)?;
    match b.get(12..16)? {
        // extended: canvas size minus one, 24-bit little-endian
        b"VP8X" => Some((1 + le24(payload.get(4..)?)?, 1 + le24(payload.get(7..)?)?)),
        b"VP8 " => {
            // lossy: dimensions follow the frame tag and sync code
            (payload.get(3..6)? == [0x9D, 0x01, 0x2A]).then_some(())?;
            Some((
                le16(payload.get(6..)?)? & 0x3FFF,
                le16(payload.get(8..)?)? & 0x3FFF,
            ))
        }
        b"VP8L" => {
            // lossless: signature byte, then 14 bits each of size minus one
            (*payload.first()? == 0x2F).then_some(())?;
            let b = payload.get(1..5)?;
            let (b1, b2, b3, b4) = (b[0] as u32, b[1] as u32, b[2] as u32, b[3] as u32);
            Some((
                1 + (b1 | (b2 & 0x3F) << 8),
                1 + (b2 >> 6 | b3 << 2 | (b4 & 0x0F) << 10),
            ))
        }
        _ => None,
    }
}

fn jpeg_dimensions(b: &[u8]) -> Option<(u32, u32)> {
    let mut i = 2; // past SOI
    loop {
        if *b.get(i)? != 0xFF {
            return None;
        }
        while *b.get(i)? == 0xFF {
            i += 1;
        }
        let marker = *b.get(i)?;
        i += 1;
        // SOF0–SOF15 carry the frame size; C4/C8/CC in that range do not
        if (0xC0..=0xCF).contains(&marker) && !matches!(marker, 0xC4 | 0xC8 | 0xCC) {
            return Some((be16(b.get(i + 5..)?)?, be16(b.get(i + 3..)?)?));
        }
        if matches!(marker, 0x01 | 0xD0..=0xD9) {
            continue; // standalone marker, no length field
        }
        i += be16(b.get(i..)?)? as usize;
    }
}

pub(crate) fn sniff_image(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        Some("image/png")
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("image/jpeg")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some("image/gif")
    } else if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        Some("image/webp")
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn nul_in_otherwise_valid_utf8_is_binary() {
        assert!(looks_binary(b"foo\0bar"));
        let payload = classify_with_home(b"foo\0bar", None);
        assert_eq!(payload.kind, Kind::Binary);
        assert_eq!(payload.formats[0].0, OCTET_STREAM);
    }

    #[test]
    fn truncated_multibyte_prefix_is_still_text() {
        let euro = "€".as_bytes();
        assert!(!prefix_looks_binary(&euro[..2]));
        assert!(prefix_looks_binary(b"caf\xe9 "));
        assert!(looks_binary(&euro[..2]));
        assert!(looks_binary(b"caf\xe9"));
    }

    #[test]
    fn human_size_rounds_down_in_one_place() {
        assert_eq!(human_size(1900), "1 KiB");
        assert_eq!(human_size(999), "999 B");
        assert_eq!(human_size(3 * 1024 * 1024 / 2), "1.5 MiB");
    }

    fn tmp() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    fn mimes(p: &Payload) -> Vec<&str> {
        p.formats.iter().map(|(m, _)| m.as_str()).collect()
    }

    fn bytes_for<'a>(p: &'a Payload, mime: &str) -> &'a [u8] {
        &p.formats.iter().find(|(m, _)| m == mime).unwrap().1
    }

    #[test]
    fn existing_file_offers_uri_list_and_text_union() {
        let dir = tmp();
        let file = dir.path().join("report.pdf");
        fs::write(&file, b"x").unwrap();

        let p = classify_with_home(file.to_str().unwrap().as_bytes(), None);
        assert_eq!(p.kind, Kind::File);
        assert_eq!(mimes(&p), vec![URI_LIST, TEXT_PLAIN]);
        assert_eq!(
            bytes_for(&p, URI_LIST),
            format!("file://{}\r\n", file.display()).as_bytes()
        );
        assert_eq!(bytes_for(&p, TEXT_PLAIN), file.to_str().unwrap().as_bytes());
    }

    #[test]
    fn surrounding_whitespace_is_trimmed_before_path_test() {
        let dir = tmp();
        let file = dir.path().join("f.txt");
        fs::write(&file, b"x").unwrap();

        let entry = format!("  {}\n", file.display());
        let p = classify_with_home(entry.as_bytes(), None);
        assert_eq!(p.kind, Kind::File);
        // the text/plain side carries the trimmed path string
        assert_eq!(bytes_for(&p, TEXT_PLAIN), file.to_str().unwrap().as_bytes());
    }

    #[test]
    fn path_with_spaces_is_percent_encoded() {
        let dir = tmp();
        let file = dir.path().join("my report v2.pdf");
        fs::write(&file, b"x").unwrap();

        let p = classify_with_home(file.to_str().unwrap().as_bytes(), None);
        let uri = String::from_utf8(bytes_for(&p, URI_LIST).to_vec()).unwrap();
        assert!(uri.contains("my%20report%20v2.pdf"), "got: {uri}");
        assert!(!uri.contains(' '));
    }

    #[test]
    fn tilde_expands_against_provided_home() {
        let home = tmp();
        fs::write(home.path().join("notes.txt"), b"x").unwrap();

        let p = classify_with_home(b"~/notes.txt", Some(home.path()));
        assert_eq!(p.kind, Kind::File);
        let uri = String::from_utf8(bytes_for(&p, URI_LIST).to_vec()).unwrap();
        assert!(uri.contains("notes.txt"));
        assert!(!uri.contains('~'));
    }

    #[test]
    fn bare_tilde_is_home_directory() {
        let home = tmp();
        let p = classify_with_home(b"~", Some(home.path()));
        assert_eq!(p.kind, Kind::File);
    }

    #[test]
    fn relative_path_stays_text_even_if_it_exists() {
        // "src" exists relative to the test cwd (project root); still text.
        assert!(Path::new("src").exists());
        let p = classify_with_home(b"src", None);
        assert_eq!(p.kind, Kind::Text);
        assert_eq!(mimes(&p), vec![TEXT_PLAIN]);
    }

    #[test]
    fn nonexistent_absolute_path_degrades_to_text() {
        let p = classify_with_home(b"/no/such/file/anywhere.txt", None);
        assert_eq!(p.kind, Kind::Text);
    }

    #[test]
    fn directory_counts_as_a_file_drop() {
        let dir = tmp();
        let p = classify_with_home(dir.path().to_str().unwrap().as_bytes(), None);
        assert_eq!(p.kind, Kind::File);
    }

    #[test]
    fn multi_line_all_existing_paths_is_multi_file() {
        let dir = tmp();
        let a = dir.path().join("a.txt");
        let b = dir.path().join("b.txt");
        fs::write(&a, b"x").unwrap();
        fs::write(&b, b"x").unwrap();

        let entry = format!("{}\n{}", a.display(), b.display());
        let p = classify_with_home(entry.as_bytes(), None);
        assert_eq!(p.kind, Kind::Files(2));
        let uri = String::from_utf8(bytes_for(&p, URI_LIST).to_vec()).unwrap();
        assert_eq!(uri.matches("\r\n").count(), 2);
        assert_eq!(uri.matches("file://").count(), 2);
    }

    #[test]
    fn multi_line_with_one_missing_path_degrades_to_text() {
        let dir = tmp();
        let a = dir.path().join("a.txt");
        fs::write(&a, b"x").unwrap();

        let entry = format!("{}\n/no/such/file.txt", a.display());
        let p = classify_with_home(entry.as_bytes(), None);
        assert_eq!(p.kind, Kind::Text);
    }

    #[test]
    fn multi_line_with_blank_interior_line_degrades_to_text() {
        let dir = tmp();
        let a = dir.path().join("a.txt");
        fs::write(&a, b"x").unwrap();

        let entry = format!("{}\n\n{}", a.display(), a.display());
        let p = classify_with_home(entry.as_bytes(), None);
        assert_eq!(p.kind, Kind::Text);
    }

    #[test]
    fn crlf_line_endings_are_handled() {
        let dir = tmp();
        let a = dir.path().join("a.txt");
        let b = dir.path().join("b.txt");
        fs::write(&a, b"x").unwrap();
        fs::write(&b, b"x").unwrap();

        let entry = format!("{}\r\n{}\r\n", a.display(), b.display());
        let p = classify_with_home(entry.as_bytes(), None);
        assert_eq!(p.kind, Kind::Files(2));
    }

    fn png_header(w: u32, h: u32) -> Vec<u8> {
        let mut b = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        b.extend_from_slice(&13u32.to_be_bytes());
        b.extend_from_slice(b"IHDR");
        b.extend_from_slice(&w.to_be_bytes());
        b.extend_from_slice(&h.to_be_bytes());
        b
    }

    #[test]
    fn png_dimensions_come_from_ihdr() {
        assert_eq!(
            image_dimensions(&png_header(2560, 1440)),
            Some((2560, 1440))
        );
        // signature without a real IHDR: still an image, no dimensions
        let bare = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 0];
        assert_eq!(image_dimensions(&bare), None);
    }

    #[test]
    fn gif_dimensions_come_from_the_screen_descriptor() {
        let mut b = b"GIF89a".to_vec();
        b.extend_from_slice(&640u16.to_le_bytes());
        b.extend_from_slice(&480u16.to_le_bytes());
        assert_eq!(image_dimensions(&b), Some((640, 480)));
        assert_eq!(image_dimensions(b"GIF89a"), None);
    }

    fn webp(chunk: &[u8; 4], payload: &[u8]) -> Vec<u8> {
        let mut b = b"RIFF\0\0\0\0WEBP".to_vec();
        b.extend_from_slice(chunk);
        b.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        b.extend_from_slice(payload);
        b
    }

    #[test]
    fn webp_dimensions_cover_all_three_layouts() {
        // VP8X: canvas 1920x1080 as size-minus-one, 24-bit LE
        let mut p = vec![0, 0, 0, 0];
        p.extend_from_slice(&[0x7F, 0x07, 0x00]); // 1919
        p.extend_from_slice(&[0x37, 0x04, 0x00]); // 1079
        assert_eq!(image_dimensions(&webp(b"VP8X", &p)), Some((1920, 1080)));

        // VP8: frame tag, sync code, then u16 dims with scaling bits masked
        let mut p = vec![0, 0, 0, 0x9D, 0x01, 0x2A];
        p.extend_from_slice(&800u16.to_le_bytes());
        p.extend_from_slice(&600u16.to_le_bytes());
        assert_eq!(image_dimensions(&webp(b"VP8 ", &p)), Some((800, 600)));

        // VP8L: signature byte then 14+14 bits of size-minus-one
        let (w, h) = (1023u32, 17u32);
        let bits = (w - 1) | (h - 1) << 14;
        let mut p = vec![0x2F];
        p.extend_from_slice(&bits.to_le_bytes());
        assert_eq!(image_dimensions(&webp(b"VP8L", &p)), Some((w, h)));
    }

    #[test]
    fn jpeg_dimensions_survive_leading_segments() {
        // SOI, an APP0 to skip, then SOF0 with height before width
        let mut b = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x04, 0, 0];
        b.extend_from_slice(&[0xFF, 0xC0, 0x00, 0x11, 0x08]);
        b.extend_from_slice(&1080u16.to_be_bytes());
        b.extend_from_slice(&1920u16.to_be_bytes());
        assert_eq!(image_dimensions(&b), Some((1920, 1080)));
        // a header cut before the frame segment yields nothing
        assert_eq!(
            image_dimensions(&[0xFF, 0xD8, 0xFF, 0xE0, 0x40, 0x00]),
            None
        );
    }

    #[test]
    fn non_images_have_no_dimensions() {
        assert_eq!(image_dimensions(b"just text"), None);
        assert_eq!(image_dimensions(&[]), None);
    }

    #[test]
    fn png_is_sniffed_not_assumed() {
        let png = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 1, 2, 3];
        let p = classify_with_home(&png, None);
        assert_eq!(p.kind, Kind::Image("image/png"));
        assert_eq!(mimes(&p), vec!["image/png"]);
        assert_eq!(bytes_for(&p, "image/png"), &png);
    }

    #[test]
    fn jpeg_gets_its_real_mime() {
        let jpg = [0xFF, 0xD8, 0xFF, 0xE0, 0, 0];
        let p = classify_with_home(&jpg, None);
        assert_eq!(p.kind, Kind::Image("image/jpeg"));
    }

    #[test]
    fn unrecognized_binary_is_octet_stream() {
        let junk = [0x00, 0xFF, 0xFE, 0x00, 0x80];
        let p = classify_with_home(&junk, None);
        assert_eq!(p.kind, Kind::Binary);
        assert_eq!(mimes(&p), vec![OCTET_STREAM]);
    }

    #[test]
    fn ordinary_prose_is_text() {
        let p = classify_with_home("just some copied words".as_bytes(), None);
        assert_eq!(p.kind, Kind::Text);
        assert_eq!(bytes_for(&p, TEXT_PLAIN), b"just some copied words");
    }

    #[test]
    fn whitespace_only_entry_is_text() {
        let p = classify_with_home(b"   \n  ", None);
        assert_eq!(p.kind, Kind::Text);
    }

    #[test]
    fn non_ascii_path_bytes_are_percent_encoded() {
        let dir = tmp();
        let file = dir.path().join("café.txt");
        fs::write(&file, b"x").unwrap();

        let p = classify_with_home(file.to_str().unwrap().as_bytes(), None);
        assert_eq!(p.kind, Kind::File);
        let uri = String::from_utf8(bytes_for(&p, URI_LIST).to_vec()).unwrap();
        assert!(uri.contains("caf%C3%A9.txt"), "got: {uri}");
    }
}
