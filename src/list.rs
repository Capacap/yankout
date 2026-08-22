//! List mode (default): recent history entries, newest first. Keyboard
//! owns selection — type to filter, arrows to move, Enter to recall —
//! and the whole window is the drag handle for the selected entry: a
//! capture-phase DragSource claims press-and-pull anywhere while clean
//! clicks fall through to the list (the M0 finding). Closes on Esc,
//! focus loss, or a clean drop.

use gtk4 as gtk;

use std::cell::{Cell, RefCell};
use std::process::ExitCode;
use std::rc::Rc;

use gtk::prelude::*;
use gtk::{gdk, gio, glib, pango};

use yankout::history::{Entry, History};
use yankout::{clipboard, interpret, provider};

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
    let store = gio::ListStore::new::<glib::BoxedAnyObject>();
    for entry in entries {
        store.append(&glib::BoxedAnyObject::new(entry.clone()));
    }

    // Lowercased once per keystroke, matched per row.
    let query = Rc::new(RefCell::new(String::new()));
    let filter = gtk::CustomFilter::new({
        let query = query.clone();
        move |obj| {
            let q = query.borrow();
            if q.is_empty() {
                return true;
            }
            let boxed = obj.downcast_ref::<glib::BoxedAnyObject>().unwrap();
            boxed.borrow::<Entry>().preview.to_lowercase().contains(&*q)
        }
    });
    let filtered = gtk::FilterListModel::new(Some(store), Some(filter.clone()));
    let selection = gtk::SingleSelection::new(Some(filtered));

    let factory = gtk::SignalListItemFactory::new();
    factory.connect_setup(|_, item| {
        let item = item.downcast_ref::<gtk::ListItem>().unwrap();
        let label = gtk::Label::builder()
            .xalign(0.0)
            .ellipsize(pango::EllipsizeMode::End)
            .margin_top(6)
            .margin_bottom(6)
            .margin_start(10)
            .margin_end(10)
            .build();
        item.set_child(Some(&label));
    });
    factory.connect_bind(|_, item| {
        let item = item.downcast_ref::<gtk::ListItem>().unwrap();
        let boxed = item.item().and_downcast::<glib::BoxedAnyObject>().unwrap();
        let label = item.child().and_downcast::<gtk::Label>().unwrap();
        label.set_text(&boxed.borrow::<Entry>().preview);
    });

    // No single_click_activate: its select-on-hover would let the pointer
    // steal the keyboard's selection (M0 finding). A plain click only
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

    let window = gtk::ApplicationWindow::builder()
        .application(app)
        .title("yankout")
        .default_width(440)
        .default_height(400)
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
            let entry = selected_entry(&selection)?;
            match backend.content(&entry.id) {
                Ok(content) => Some(provider::content_provider(&interpret::interpret(&content))),
                Err(e) => {
                    eprintln!("yankout: {e}");
                    None
                }
            }
        }
    });

    // drag-end fires after every drag; drag-cancel fires first on failure.
    // Only a clean drop closes the window — a cancelled drag keeps it up.
    // The dragging flag holds off the focus-loss close while a drag is in
    // flight, since the drop target may take focus before drag-end lands.
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
            if !cancelled.replace(false) {
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
    // whichever widget holds focus.
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
                if let Some(entry) = selected_entry(&selection) {
                    recall(backend.as_ref(), &entry, &win);
                }
            } else if search.has_focus() {
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
        let query = query.clone();
        let selection = selection.clone();
        let list = list.clone();
        move |s| {
            *query.borrow_mut() = s.text().to_lowercase();
            filter.changed(gtk::FilterChange::Different);
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
            if let Some(obj) = selection.item(pos) {
                let entry = obj
                    .downcast_ref::<glib::BoxedAnyObject>()
                    .unwrap()
                    .borrow::<Entry>()
                    .clone();
                recall(backend.as_ref(), &entry, &win);
            }
        }
    });

    window.present();
    search.grab_focus();
}

fn selected_entry(selection: &gtk::SingleSelection) -> Option<Entry> {
    let obj = selection.selected_item()?;
    let boxed = obj.downcast_ref::<glib::BoxedAnyObject>()?;
    Some(boxed.borrow::<Entry>().clone())
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
fn recall(backend: &dyn History, entry: &Entry, window: &gtk::ApplicationWindow) {
    match backend
        .content(&entry.id)
        .and_then(|content| clipboard::write(&content))
    {
        Ok(()) => window.close(),
        Err(e) => eprintln!("yankout: recall failed: {e}"),
    }
}
