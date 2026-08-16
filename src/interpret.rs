//! Drag-time classification: decide what a clipboard entry *is* and build
//! the payload a drop target can consume. Pure Rust, no GTK types — the
//! GdkContentProvider is a thin byte-backed adapter over [`Payload`] built
//! elsewhere, which keeps this whole module testable without a display.

use std::path::{Path, PathBuf};

pub const URI_LIST: &str = "text/uri-list";
pub const TEXT_PLAIN: &str = "text/plain;charset=utf-8";
pub const OCTET_STREAM: &str = "application/octet-stream";

/// What an entry turned out to be. Drives type markers and the puck label,
/// not the offered formats — those live in [`Payload::formats`].
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

pub fn interpret(content: &[u8]) -> Payload {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    interpret_with_home(content, home.as_deref())
}

/// `home` backs `~` expansion; injectable so tests control the filesystem.
pub fn interpret_with_home(content: &[u8], home: Option<&Path>) -> Payload {
    if let Some(mime) = sniff_image(content) {
        return Payload {
            kind: Kind::Image(mime),
            formats: vec![(mime.to_string(), content.to_vec())],
        };
    }

    let Ok(text) = std::str::from_utf8(content) else {
        return Payload {
            kind: Kind::Binary,
            formats: vec![(OCTET_STREAM.to_string(), content.to_vec())],
        };
    };

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

fn sniff_image(bytes: &[u8]) -> Option<&'static str> {
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

        let p = interpret_with_home(file.to_str().unwrap().as_bytes(), None);
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
        let p = interpret_with_home(entry.as_bytes(), None);
        assert_eq!(p.kind, Kind::File);
        // the text/plain side carries the trimmed path string
        assert_eq!(bytes_for(&p, TEXT_PLAIN), file.to_str().unwrap().as_bytes());
    }

    #[test]
    fn path_with_spaces_is_percent_encoded() {
        let dir = tmp();
        let file = dir.path().join("my report v2.pdf");
        fs::write(&file, b"x").unwrap();

        let p = interpret_with_home(file.to_str().unwrap().as_bytes(), None);
        let uri = String::from_utf8(bytes_for(&p, URI_LIST).to_vec()).unwrap();
        assert!(uri.contains("my%20report%20v2.pdf"), "got: {uri}");
        assert!(!uri.contains(' '));
    }

    #[test]
    fn tilde_expands_against_provided_home() {
        let home = tmp();
        fs::write(home.path().join("notes.txt"), b"x").unwrap();

        let p = interpret_with_home(b"~/notes.txt", Some(home.path()));
        assert_eq!(p.kind, Kind::File);
        let uri = String::from_utf8(bytes_for(&p, URI_LIST).to_vec()).unwrap();
        assert!(uri.contains("notes.txt"));
        assert!(!uri.contains('~'));
    }

    #[test]
    fn bare_tilde_is_home_directory() {
        let home = tmp();
        let p = interpret_with_home(b"~", Some(home.path()));
        assert_eq!(p.kind, Kind::File);
    }

    #[test]
    fn relative_path_stays_text_even_if_it_exists() {
        // "src" exists relative to the test cwd (project root); still text.
        assert!(Path::new("src").exists());
        let p = interpret_with_home(b"src", None);
        assert_eq!(p.kind, Kind::Text);
        assert_eq!(mimes(&p), vec![TEXT_PLAIN]);
    }

    #[test]
    fn nonexistent_absolute_path_degrades_to_text() {
        let p = interpret_with_home(b"/no/such/file/anywhere.txt", None);
        assert_eq!(p.kind, Kind::Text);
    }

    #[test]
    fn directory_counts_as_a_file_drop() {
        let dir = tmp();
        let p = interpret_with_home(dir.path().to_str().unwrap().as_bytes(), None);
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
        let p = interpret_with_home(entry.as_bytes(), None);
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
        let p = interpret_with_home(entry.as_bytes(), None);
        assert_eq!(p.kind, Kind::Text);
    }

    #[test]
    fn multi_line_with_blank_interior_line_degrades_to_text() {
        let dir = tmp();
        let a = dir.path().join("a.txt");
        fs::write(&a, b"x").unwrap();

        let entry = format!("{}\n\n{}", a.display(), a.display());
        let p = interpret_with_home(entry.as_bytes(), None);
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
        let p = interpret_with_home(entry.as_bytes(), None);
        assert_eq!(p.kind, Kind::Files(2));
    }

    #[test]
    fn png_is_sniffed_not_assumed() {
        let png = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 1, 2, 3];
        let p = interpret_with_home(&png, None);
        assert_eq!(p.kind, Kind::Image("image/png"));
        assert_eq!(mimes(&p), vec!["image/png"]);
        assert_eq!(bytes_for(&p, "image/png"), &png);
    }

    #[test]
    fn jpeg_gets_its_real_mime() {
        let jpg = [0xFF, 0xD8, 0xFF, 0xE0, 0, 0];
        let p = interpret_with_home(&jpg, None);
        assert_eq!(p.kind, Kind::Image("image/jpeg"));
    }

    #[test]
    fn unrecognized_binary_is_octet_stream() {
        let junk = [0x00, 0xFF, 0xFE, 0x00, 0x80];
        let p = interpret_with_home(&junk, None);
        assert_eq!(p.kind, Kind::Binary);
        assert_eq!(mimes(&p), vec![OCTET_STREAM]);
    }

    #[test]
    fn ordinary_prose_is_text() {
        let p = interpret_with_home("just some copied words".as_bytes(), None);
        assert_eq!(p.kind, Kind::Text);
        assert_eq!(bytes_for(&p, TEXT_PLAIN), b"just some copied words");
    }

    #[test]
    fn whitespace_only_entry_is_text() {
        let p = interpret_with_home(b"   \n  ", None);
        assert_eq!(p.kind, Kind::Text);
    }

    #[test]
    fn non_ascii_path_bytes_are_percent_encoded() {
        let dir = tmp();
        let file = dir.path().join("café.txt");
        fs::write(&file, b"x").unwrap();

        let p = interpret_with_home(file.to_str().unwrap().as_bytes(), None);
        assert_eq!(p.kind, Kind::File);
        let uri = String::from_utf8(bytes_for(&p, URI_LIST).to_vec()).unwrap();
        assert!(uri.contains("caf%C3%A9.txt"), "got: {uri}");
    }
}
