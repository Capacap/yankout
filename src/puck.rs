//! Puck mode (`--current`): a postage-stamp window whose whole surface
//! drags the live clipboard. Exits on a clean drop or Esc; immune to
//! focus loss, because the working rhythm is spawn, click into the
//! target to scroll its drop zone into view, come back and drag.

use gtk4 as gtk;

use std::cell::Cell;
use std::process::ExitCode;
use std::rc::Rc;

use gtk::prelude::*;
use gtk::{gdk, glib};

use yankout::interpret::{self, Kind, Payload};
use yankout::{clipboard, provider};

pub fn run(user_css: Option<String>) -> ExitCode {
    let content = match clipboard::read() {
        Ok(content) => content,
        Err(e) => {
            eprintln!("yankout: {e}");
            return ExitCode::FAILURE;
        }
    };
    let payload = interpret::interpret(&content);

    // Modes get distinct application ids: under a shared id GApplication
    // uniqueness would make a `yankout` launch re-present an open puck
    // instead of showing the list, and compositor rules couldn't tell the
    // windows apart.
    let app = gtk::Application::builder()
        .application_id("dev.yankout.puck")
        .build();
    app.connect_activate(move |app| {
        crate::theme::apply(user_css.as_deref());
        build_puck(app, &payload);
    });
    // GApplication must not see our argv: it would reject --current.
    let code: i32 = app.run_with_args::<&str>(&[]).into();
    ExitCode::from(code as u8)
}

fn build_puck(app: &gtk::Application, payload: &Payload) {
    let kind_label = gtk::Label::builder()
        .label(kind_text(&payload.kind))
        .css_classes(["puck-kind"])
        .build();
    let detail_label = gtk::Label::builder()
        .label(detail_text(payload))
        .css_classes(["puck-detail"])
        .build();

    // One line, like a list row: `kind  detail`.
    let row = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(12)
        .css_classes(["puck"])
        .build();
    row.append(&kind_label);
    row.append(&detail_label);

    let window = gtk::ApplicationWindow::builder()
        .application(app)
        .title("yankout")
        .resizable(false)
        .child(&row)
        .build();

    let source = gtk::DragSource::new();
    source.set_actions(gdk::DragAction::COPY);
    source.set_propagation_phase(gtk::PropagationPhase::Capture);
    let drag_payload = payload.clone();
    source.connect_prepare(move |_, _, _| Some(provider::content_provider(&drag_payload)));

    // drag-end fires after every drag; drag-cancel fires first on failure.
    // Only a clean drop closes the puck — a cancelled drag keeps it up for
    // another try.
    let cancelled = Rc::new(Cell::new(false));
    let flag = cancelled.clone();
    source.connect_drag_cancel(move |_, _, _| {
        flag.set(true);
        false
    });
    let win = window.clone();
    source.connect_drag_end(move |_, _, _| {
        if !cancelled.replace(false) {
            win.close();
        }
    });
    window.add_controller(source);

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

fn kind_text(kind: &Kind) -> String {
    match kind {
        Kind::File => "file".into(),
        Kind::Files(n) => format!("{n} files"),
        Kind::Image(mime) => (*mime).into(),
        Kind::Binary => "binary".into(),
        Kind::Text => "text".into(),
    }
}

fn detail_text(payload: &Payload) -> String {
    match &payload.kind {
        // For a path, the tail is the informative end.
        Kind::File => {
            let text = payload_text(payload);
            truncate_keeping_tail(text.lines().next().unwrap_or(""), 36)
        }
        Kind::Files(_) | Kind::Text => {
            let text = payload_text(payload);
            truncate(text.lines().next().unwrap_or(""), 36)
        }
        Kind::Image(_) | Kind::Binary => {
            interpret::human_size(payload.formats.first().map_or(0, |(_, b)| b.len() as u64))
        }
    }
}

fn payload_text(payload: &Payload) -> String {
    payload
        .formats
        .iter()
        .find(|(mime, _)| mime == interpret::TEXT_PLAIN)
        .map(|(_, bytes)| String::from_utf8_lossy(bytes).into_owned())
        .unwrap_or_default()
}

fn truncate(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        return s.to_string();
    }
    let head: String = s.chars().take(max_chars).collect();
    format!("{head}…")
}

fn truncate_keeping_tail(s: &str, max_chars: usize) -> String {
    let count = s.chars().count();
    if count <= max_chars {
        return s.to_string();
    }
    let tail: String = s.chars().skip(count - max_chars).collect();
    format!("…{tail}")
}
