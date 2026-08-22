//! Which of a selection's offered types to take. Used at both capture
//! points (the watcher and the puck's `wl-paste`) so they cannot drift:
//! what history stores is what a drag of the live clipboard would take.

/// Set by password managers on offers that must not be retained.
pub const PASSWORD_HINT: &str = "x-kde-passwordManagerHint";

/// Image encodings drag-time classification can recognise by magic
/// bytes; png first because it is lossless.
const SNIFFABLE_IMAGES: &[&str] = &["image/png", "image/jpeg", "image/gif", "image/webp"];

/// Plain-text types in preference order; the legacy names are what
/// older X-lineage apps offer instead of a MIME type.
const TEXT_TYPES: &[&str] = &[
    "text/plain;charset=utf-8",
    "text/plain",
    "UTF8_STRING",
    "TEXT",
    "STRING",
];

/// The type to read, or `None` when nothing offered is worth keeping.
///
/// A recognisable image beats text, text beats an image encoding the
/// classifier cannot name (svg, bmp, tiff, xcf, …): stored bytes carry
/// no type, so such an image would later surface as an opaque blob
/// where the text alternative is at least honest. Anything else is
/// not clipboard history in this tool's sense.
pub fn pick<'a>(offered: impl IntoIterator<Item = &'a str> + Clone) -> Option<&'a str> {
    let has = |wanted: &str| offered.clone().into_iter().find(|m| *m == wanted);
    if has(PASSWORD_HINT).is_some() {
        return None;
    }
    SNIFFABLE_IMAGES
        .iter()
        .chain(TEXT_TYPES)
        .find_map(|t| has(t))
        .or_else(|| offered.into_iter().find(|m| m.starts_with("image/")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn png_beats_other_images_beats_text() {
        assert_eq!(
            pick(["text/plain", "image/webp", "image/png"]),
            Some("image/png")
        );
        assert_eq!(pick(["text/plain", "image/webp"]), Some("image/webp"));
        assert_eq!(
            pick(["text/plain", "text/plain;charset=utf-8"]),
            Some("text/plain;charset=utf-8")
        );
    }

    #[test]
    fn text_beats_an_image_the_classifier_cannot_name() {
        assert_eq!(pick(["image/svg+xml", "text/plain"]), Some("text/plain"));
        assert_eq!(pick(["image/x-xcf"]), Some("image/x-xcf"));
    }

    #[test]
    fn legacy_text_names_are_accepted() {
        assert_eq!(pick(["STRING", "UTF8_STRING"]), Some("UTF8_STRING"));
    }

    #[test]
    fn password_hint_suppresses_the_whole_offer() {
        assert_eq!(pick(["text/plain", PASSWORD_HINT]), None);
    }

    #[test]
    fn unknown_types_are_not_history() {
        assert_eq!(pick(["application/pdf"]), None);
        assert_eq!(pick([]), None);
    }
}
