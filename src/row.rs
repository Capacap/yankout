//! A history entry as a GObject, so the list model can filter on its
//! `preview` property through a plain `StringFilter` expression instead
//! of closures over downcasts.

use gtk4 as gtk;

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
    pub fn new(entry: &Entry) -> Self {
        glib::Object::builder()
            .property("id", &entry.id)
            .property("preview", &entry.preview)
            .build()
    }

    pub fn entry(&self) -> Entry {
        Entry {
            id: self.id(),
            preview: self.preview(),
        }
    }
}
