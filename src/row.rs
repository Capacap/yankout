//! A history entry as a GObject, so the list model can filter on its
//! `preview` property through a plain `StringFilter` expression instead
//! of closures over downcasts. `age` is preformatted display text, kept
//! out of `preview` so the filter never matches it.

use gtk4 as gtk;

use std::time::SystemTime;

use gtk::glib;
use gtk::prelude::*;
use gtk::subclass::prelude::*;

use yankout_core::history::Entry;

mod imp {
    use super::*;
    use std::cell::RefCell;

    #[derive(Default, glib::Properties)]
    #[properties(wrapper_type = super::Row)]
    pub struct Row {
        #[property(get, set)]
        pub id: RefCell<String>,
        #[property(get, set)]
        pub preview: RefCell<String>,
        #[property(get, set)]
        pub age: RefCell<String>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Row {
        const NAME: &'static str = "YankoutRow";
        type Type = super::Row;
    }

    #[glib::derived_properties]
    impl ObjectImpl for Row {}
}

glib::wrapper! {
    pub struct Row(ObjectSubclass<imp::Row>);
}

impl Row {
    pub fn new(entry: &Entry, now: SystemTime) -> Self {
        glib::Object::builder()
            .property("id", &entry.id)
            .property("preview", &entry.preview)
            .property(
                "age",
                entry.stamp.map(|s| age_text(s, now)).unwrap_or_default(),
            )
            .build()
    }
}

/// Largest whole unit only: the age answers "how long ago", not "when".
fn age_text(stamp: SystemTime, now: SystemTime) -> String {
    // a stamp in the future is clock skew; call it fresh
    let secs = now.duration_since(stamp).unwrap_or_default().as_secs();
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 60 * 60 {
        format!("{}m", secs / 60)
    } else if secs < 24 * 60 * 60 {
        format!("{}h", secs / (60 * 60))
    } else {
        format!("{}d", secs / (24 * 60 * 60))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn ages_use_the_largest_whole_unit() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000_000);
        let at = |secs| age_text(now - Duration::from_secs(secs), now);
        assert_eq!(at(0), "0s");
        assert_eq!(at(59), "59s");
        assert_eq!(at(60), "1m");
        assert_eq!(at(59 * 60 + 59), "59m");
        assert_eq!(at(60 * 60), "1h");
        assert_eq!(at(23 * 60 * 60 + 59 * 60), "23h");
        assert_eq!(at(24 * 60 * 60), "1d");
        assert_eq!(at(40 * 24 * 60 * 60), "40d");
    }

    #[test]
    fn future_stamps_read_as_fresh() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000_000);
        assert_eq!(age_text(now + Duration::from_secs(300), now), "0s");
    }
}
