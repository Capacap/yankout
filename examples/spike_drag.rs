//! M0 spike 1: drag a hardcoded string and a hardcoded file path out of a
//! GTK4 window into a browser / file manager. Throwaway.

use gtk4 as gtk;

use gtk::prelude::*;
use gtk::{gdk, glib};

const FILE_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml");

fn main() -> glib::ExitCode {
    let app = gtk::Application::builder()
        .application_id("dev.capacap.yankout.spike-drag")
        .build();
    app.connect_activate(build_ui);
    app.run()
}

fn build_ui(app: &gtk::Application) {
    let string_tile = drag_tile("drag me: plain string", || {
        gdk::ContentProvider::for_value(&"hello from yankout spike".to_value())
    });

    let file_tile = drag_tile("drag me: file path", || {
        let uris = format!("file://{FILE_PATH}\r\n");
        gdk::ContentProvider::new_union(&[
            gdk::ContentProvider::for_bytes(
                "text/uri-list",
                &glib::Bytes::from_owned(uris.into_bytes()),
            ),
            gdk::ContentProvider::for_value(&FILE_PATH.to_value()),
        ])
    });

    let vbox = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(8)
        .margin_top(12)
        .margin_bottom(12)
        .margin_start(12)
        .margin_end(12)
        .build();
    vbox.append(&string_tile);
    vbox.append(&file_tile);

    let window = gtk::ApplicationWindow::builder()
        .application(app)
        .title("yankout spike: drag out")
        .default_width(280)
        .child(&vbox)
        .build();

    let key = gtk::EventControllerKey::new();
    let win = window.clone();
    key.connect_key_pressed(move |_, keyval, _, _| {
        if keyval == gdk::Key::Escape {
            win.close();
            glib::Propagation::Stop
        } else {
            glib::Propagation::Proceed
        }
    });
    window.add_controller(key);

    window.present();
}

fn drag_tile(
    label: &str,
    provider: impl Fn() -> gdk::ContentProvider + 'static,
) -> gtk::Frame {
    let child = gtk::Label::builder()
        .label(label)
        .margin_top(16)
        .margin_bottom(16)
        .margin_start(16)
        .margin_end(16)
        .build();

    let source = gtk::DragSource::new();
    source.set_actions(gdk::DragAction::COPY);
    source.connect_prepare(move |_, _, _| {
        println!("prepare");
        Some(provider())
    });
    source.connect_drag_end(|_, _, _| println!("drag-end (drop landed)"));
    source.connect_drag_cancel(|_, _, reason| {
        println!("drag-cancel: {reason:?}");
        false
    });
    child.add_controller(source);

    gtk::Frame::builder().child(&child).build()
}
