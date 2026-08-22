//! The thin GTK adapter over [`yankout_core::interpret::Payload`]: everything
//! interesting about a payload is decided and byte-assembled in pure code;
//! this only wraps it for GDK.

use gtk4::glib::value::ToValue as _;
use gtk4::{gdk, glib};

use yankout_core::interpret::{Payload, TEXT_PLAIN};

pub fn content_provider(payload: &Payload) -> gdk::ContentProvider {
    let providers: Vec<gdk::ContentProvider> = payload
        .formats
        .iter()
        .map(|(mime, bytes)| {
            // Text goes through for_value so GDK's serializers offer every
            // text flavor a receiver might ask for, not just our one MIME.
            if mime == TEXT_PLAIN
                && let Ok(text) = std::str::from_utf8(bytes)
            {
                return gdk::ContentProvider::for_value(&text.to_value());
            }
            gdk::ContentProvider::for_bytes(mime, &glib::Bytes::from(bytes.as_slice()))
        })
        .collect();

    match providers.len() {
        1 => providers.into_iter().next().expect("len checked"),
        _ => gdk::ContentProvider::new_union(&providers),
    }
}
