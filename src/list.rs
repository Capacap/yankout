//! List mode (default): recent history entries, newest first. Keyboard
//! owns selection — type to filter, arrows to move, Enter to recall —
//! and the whole window is the drag handle for the selected entry: a
//! capture-phase DragSource claims press-and-pull anywhere while clean
//! clicks fall through to the list. Closes on Esc,
//! focus loss, or a clean drop.

use gtk4 as gtk;

use std::cell::Cell;
use std::process::ExitCode;
use std::rc::Rc;

use gtk::prelude::*;
use gtk::{gdk, gio, glib, pango};

use yankout_core::history::{Entry, History};
use yankout_core::{clipboard, payload};

use crate::provider;
use crate::row::Row;

pub fn run(user_css: Option<String>, backend: Box<dyn History>) -> ExitCode {
    let backend: Rc<dyn History> = Rc::from(backend);
    let entries = match backend.entries() {
        Ok(entries) => entries,
        Err(e) => {
            eprintln!("yankout: {e}");
            return ExitCode::FAILURE;
        }
    };

    // Distinct from the puck's id — see the comment in puck.rs.
    let app = gtk::Application::builder()
        .application_id("dev.yankout.list")
        .build();
    app.connect_activate(move |app| {
        crate::theme::apply(user_css.as_deref());
        build_list(app, backend.clone(), &entries);
    });
    // GApplication must not see our argv.
    let code: i32 = app.run_with_args::<&str>(&[]).into();
    ExitCode::from(code as u8)
}

fn build_list(app: &gtk::Application, backend: Rc<dyn History>, entries: &[Entry]) {
    let (filter, selection) = model(entries);
    // cliphist keeps no times; rather than a blank column, no column
    let show_age = entries.iter().any(|e| e.stamp.is_some());

    let factory = gtk::SignalListItemFactory::new();
    factory.connect_setup(move |_, item| {
        let item = item.downcast_ref::<gtk::ListItem>().unwrap();
        let hbox = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(8)
            .margin_top(6)
            .margin_bottom(6)
            .margin_start(10)
            .margin_end(10)
            .build();
        if show_age {
            let age = gtk::Label::builder()
                .xalign(1.0)
                .width_chars(3)
                .css_classes(["age"])
                .build();
            hbox.append(&age);
        }
        let preview = gtk::Label::builder()
            .xalign(0.0)
            .ellipsize(pango::EllipsizeMode::End)
            .hexpand(true)
            .build();
        hbox.append(&preview);
        item.set_child(Some(&hbox));
    });
    factory.connect_bind(move |_, item| {
        let item = item.downcast_ref::<gtk::ListItem>().unwrap();
        let row = item.item().and_downcast::<Row>().unwrap();
        let hbox = item.child().and_downcast::<gtk::Box>().unwrap();
        let mut child = hbox.first_child();
        if show_age {
            let age = child.and_downcast::<gtk::Label>().unwrap();
            age.set_text(&row.age());
            child = age.next_sibling();
        }
        let preview = child.and_downcast::<gtk::Label>().unwrap();
        preview.set_text(&row.preview());
    });

    // No single_click_activate: its select-on-hover would let the pointer
    // steal the keyboard's selection. A plain click only
    // selects; activate is Enter or double-click.
    let list = gtk::ListView::new(Some(selection.clone()), Some(factory));
    list.add_css_class("history");

    // A plain Entry, not SearchEntry: no magnifier or clear icons, so the
    // default theme can render it as a bare prompt line.
    let search = gtk::Entry::builder()
        .placeholder_text("filter")
        .hexpand(true)
        .css_classes(["filter"])
        .build();
    let prompt = gtk::Label::builder()
        .label(">")
        .css_classes(["prompt"])
        .build();
    let bar = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .css_classes(["bar"])
        .build();
    bar.append(&prompt);
    bar.append(&search);

    let vbox = gtk::Box::new(gtk::Orientation::Vertical, 0);
    if entries.is_empty() {
        let empty = gtk::Label::builder()
            .label("history empty")
            .css_classes(["empty"])
            .vexpand(true)
            .build();
        vbox.append(&empty);
    } else {
        let scroller = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vexpand(true)
            .child(&list)
            .build();
        vbox.append(&bar);
        vbox.append(&scroller);
    }

    // Fixed size makes GTK report min == max, which the fixed-size
    // heuristics in niri and sway float without a window rule.
    let window = gtk::ApplicationWindow::builder()
        .application(app)
        .title("yankout")
        .width_request(440)
        .height_request(400)
        .resizable(false)
        .child(&vbox)
        .build();

    // Decoding runs in prepare, so only the entry actually dragged is read.
    let source = gtk::DragSource::new();
    source.set_actions(gdk::DragAction::COPY);
    source.set_propagation_phase(gtk::PropagationPhase::Capture);
    source.connect_prepare({
        let selection = selection.clone();
        let backend = backend.clone();
        move |_, _, _| {
            let id = selected_id(&selection)?;
            match backend.content(&id) {
                Ok(content) => Some(provider::content_provider(&payload::classify(&content))),
                Err(e) => {
                    eprintln!("yankout: {e}");
                    None
                }
            }
        }
    });

    // drag-end fires after every drag; drag-cancel fires first on failure.
    // A clean drop closes the window; a cancelled drag keeps it up only
    // if it still has focus, since the focus-loss notify that fired while
    // the drag was in flight was deliberately ignored and will not repeat.
    let dragging = Rc::new(Cell::new(false));
    let cancelled = Rc::new(Cell::new(false));
    source.connect_drag_begin({
        let dragging = dragging.clone();
        move |_, _| dragging.set(true)
    });
    source.connect_drag_cancel({
        let cancelled = cancelled.clone();
        move |_, _, _| {
            cancelled.set(true);
            false
        }
    });
    source.connect_drag_end({
        let dragging = dragging.clone();
        let win = window.clone();
        move |_, _, _| {
            dragging.set(false);
            if !cancelled.replace(false) || !win.is_active() {
                win.close();
            }
        }
    });
    window.add_controller(source);

    window.connect_is_active_notify({
        let dragging = dragging.clone();
        move |win| {
            if !win.is_active() && !dragging.get() {
                win.close();
            }
        }
    });

    // Capture phase so navigation wins over both the list and the entry;
    // everything else is forwarded to the entry and becomes filter text,
    // whichever widget holds focus. With no history the entry is not in
    // the window at all, so nothing is forwarded to it.
    let has_entries = !entries.is_empty();
    let key = gtk::EventControllerKey::new();
    key.set_propagation_phase(gtk::PropagationPhase::Capture);
    key.connect_key_pressed({
        let win = window.clone();
        let selection = selection.clone();
        let list = list.clone();
        let backend = backend.clone();
        let search = search.clone();
        move |controller, keyval, _, state| {
            let ctrl = state.contains(gdk::ModifierType::CONTROL_MASK);
            if keyval == gdk::Key::Escape {
                win.close();
            } else if keyval == gdk::Key::Down
                || (ctrl && (keyval == gdk::Key::n || keyval == gdk::Key::j))
            {
                move_selection(&selection, &list, 1);
            } else if keyval == gdk::Key::Up
                || (ctrl && (keyval == gdk::Key::p || keyval == gdk::Key::k))
            {
                move_selection(&selection, &list, -1);
            } else if keyval == gdk::Key::Return || keyval == gdk::Key::KP_Enter {
                if let Some(id) = selected_id(&selection) {
                    recall(backend.as_ref(), &id, &win);
                }
            } else if !has_entries || search.has_focus() {
                return glib::Propagation::Proceed;
            } else {
                search.grab_focus_without_selecting();
                return match controller.forward(&search) {
                    true => glib::Propagation::Stop,
                    false => glib::Propagation::Proceed,
                };
            }
            glib::Propagation::Stop
        }
    });
    window.add_controller(key);

    search.connect_changed({
        let selection = selection.clone();
        let list = list.clone();
        move |s| {
            filter.set_search(Some(s.text().as_str()));
            if selection.n_items() > 0 {
                selection.set_selected(0);
                list.scroll_to(0, gtk::ListScrollFlags::NONE, None);
            }
        }
    });

    // Double-click recall; Enter is already handled window-wide above.
    list.connect_activate({
        let backend = backend.clone();
        let win = window.clone();
        let selection = selection.clone();
        move |_, pos| {
            if let Some(row) = selection.item(pos).and_downcast::<Row>() {
                recall(backend.as_ref(), &row.id(), &win);
            }
        }
    });

    window.present();
    if has_entries {
        search.grab_focus();
    }
}

/// Rows behind a case-insensitive substring filter on the preview.
fn model(entries: &[Entry]) -> (gtk::StringFilter, gtk::SingleSelection) {
    let now = std::time::SystemTime::now();
    let store = entries
        .iter()
        .map(|e| Row::new(e, now))
        .collect::<gio::ListStore>();
    let filter = gtk::StringFilter::builder()
        .expression(Row::this_expression("preview"))
        .match_mode(gtk::StringFilterMatchMode::Substring)
        .ignore_case(true)
        .build();
    let filtered = gtk::FilterListModel::new(Some(store), Some(filter.clone()));
    (filter, gtk::SingleSelection::new(Some(filtered)))
}

fn selected_id(selection: &gtk::SingleSelection) -> Option<String> {
    Some(selection.selected_item().and_downcast::<Row>()?.id())
}

fn move_selection(selection: &gtk::SingleSelection, list: &gtk::ListView, delta: i32) {
    let n = selection.n_items();
    if n == 0 {
        return;
    }
    let current = match selection.selected() {
        gtk::INVALID_LIST_POSITION => 0,
        pos => pos,
    };
    let next = current.saturating_add_signed(delta).min(n - 1);
    selection.set_selected(next);
    list.scroll_to(next, gtk::ListScrollFlags::NONE, None);
}

/// The recall verb: promote the entry back to the active clipboard. Only
/// success closes the window; a failure is worth leaving it up for.
fn recall(backend: &dyn History, id: &str, window: &gtk::ApplicationWindow) {
    match backend
        .content(id)
        .and_then(|content| clipboard::write(&content))
    {
        Ok(()) => window.close(),
        Err(e) => eprintln!("yankout: recall failed: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, preview: &str) -> Entry {
        Entry {
            id: id.into(),
            preview: preview.into(),
            stamp: None,
        }
    }

    #[test]
    fn filter_is_a_case_insensitive_substring_match() {
        // GTK types need a display; without one there is nothing to test here.
        if gtk::init().is_err() {
            eprintln!("skipped: no display");
            return;
        }
        let entries = [
            entry("1", "Hello World"),
            entry("2", "cargo build"),
            entry("3", "hello again"),
        ];
        let (filter, selection) = model(&entries);
        assert_eq!(selection.n_items(), 3);
        filter.set_search(Some("HELLO"));
        assert_eq!(selection.n_items(), 2);
        assert_eq!(selected_id(&selection).unwrap(), "1");
        filter.set_search(Some("build"));
        assert_eq!(selection.n_items(), 1);
        assert_eq!(selected_id(&selection).unwrap(), "2");
        filter.set_search(Some("zzz"));
        assert_eq!(selection.n_items(), 0);
        assert_eq!(selected_id(&selection), None);
        filter.set_search(None);
        assert_eq!(selection.n_items(), 3);
    }
}
