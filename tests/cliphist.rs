//! Integration tests for the cliphist backend against a scratch database
//! (cliphist's -db-path flag). Requires the cliphist binary on PATH; the
//! live clipboard and the real database are never touched.

use std::path::PathBuf;
use std::process::{Command, Stdio};

use yankout::history::{Cliphist, History};
use yankout::interpret::{interpret_with_home, Kind};

// A valid 1x1 PNG: cliphist only produces its "[[ binary data ... ]]"
// preview when it can fully parse the image, so a bare magic-number
// prefix is not enough here.
const PNG: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
    0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x00, 0x00, 0x00, 0x00, 0x3A,
    0x7E, 0x9B, 0x55, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x60,
    0x00, 0x00, 0x00, 0x02, 0x00, 0x01, 0x48, 0xAF, 0xA4, 0x71, 0x00, 0x00, 0x00, 0x00, 0x49,
    0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
];

fn scratch() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("db");
    (dir, db)
}

fn store(db: &PathBuf, content: &[u8]) {
    use std::io::Write as _;
    let mut child = Command::new("cliphist")
        .arg("-db-path")
        .arg(db)
        .arg("store")
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(content).unwrap();
    assert!(child.wait().unwrap().success());
}

#[test]
fn empty_database_is_an_empty_list_not_an_error() {
    let (_dir, db) = scratch();
    let backend = Cliphist::custom("cliphist", Some(db));
    assert_eq!(backend.entries().unwrap(), vec![]);
}

#[test]
fn entries_come_back_newest_first_with_content_roundtrip() {
    let (_dir, db) = scratch();
    store(&db, b"first entry");
    store(&db, b"second entry");

    let backend = Cliphist::custom("cliphist", Some(db));
    let entries = backend.entries().unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].preview, "second entry");
    assert_eq!(entries[1].preview, "first entry");

    assert_eq!(backend.content(&entries[0].id).unwrap(), b"second entry");
    assert_eq!(backend.content(&entries[1].id).unwrap(), b"first entry");
}

#[test]
fn binary_survives_the_roundtrip_and_classifies_as_image() {
    let (_dir, db) = scratch();
    store(&db, PNG);

    let backend = Cliphist::custom("cliphist", Some(db));
    let entries = backend.entries().unwrap();
    assert!(entries[0].preview.contains("binary"), "{}", entries[0].preview);

    let content = backend.content(&entries[0].id).unwrap();
    assert_eq!(content, PNG);
    assert_eq!(
        interpret_with_home(&content, None).kind,
        Kind::Image("image/png")
    );
}

#[test]
fn absent_binary_is_a_clear_error() {
    let backend = Cliphist::custom("definitely-not-cliphist-zzz", None);
    let err = backend.entries().unwrap_err();
    assert!(err.0.contains("not found"), "{}", err.0);
}

#[test]
fn decode_of_unknown_id_is_an_error() {
    let (_dir, db) = scratch();
    store(&db, b"something");
    let backend = Cliphist::custom("cliphist", Some(db));
    assert!(backend.content("99999").is_err());
}
