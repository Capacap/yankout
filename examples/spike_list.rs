//! M0 spike 2: the design's riskiest mechanism — a capture-phase
//! window-level GtkDragSource over a single_click_activate ListView.
//! Questions this answers by hand-testing:
//!   - does a clean click still reach the ListView (activate = recall)?
//!   - does press-and-pull anywhere start a drag of the selected row?
//!   - does pressing on a row select it before the drag claims the press?
//! Throwaway.

use gtk4 as gtk;

use gtk::prelude::*;
use gtk::{gdk, glib};

fn main() -> glib::ExitCode {
    let app = gtk::Application::builder()
        .application_id("dev.capacap.yankout.spike-list")
        .build();
    app.connect_activate(build_ui);
    app.run()
}

fn build_ui(app: &gtk::Application) {
    let items: Vec<String> = (0..10).map(|i| format!("history entry {i}")).collect();
    let refs: Vec<&str> = items.iter().map(String::as_str).collect();
    let model = gtk::StringList::new(&refs);
    let selection = gtk::SingleSelection::new(Some(model));
    selection.connect_selected_notify(|s| println!("selected: row {}", s.selected()));

    let factory = gtk::SignalListItemFactory::new();
    factory.connect_setup(|_, item| {
        let item = item.downcast_ref::<gtk::ListItem>().unwrap();
        let label = gtk::Label::builder()
            .xalign(0.0)
            .margin_top(6)
            .margin_bottom(6)
            .margin_start(10)
            .margin_end(10)
            .build();
        item.set_child(Some(&label));
    });
    factory.connect_bind(|_, item| {
        let item = item.downcast_ref::<gtk::ListItem>().unwrap();
        let obj = item.item().and_downcast::<gtk::StringObject>().unwrap();
        let label = item.child().and_downcast::<gtk::Label>().unwrap();
        label.set_text(&obj.string());
    });

    let list = gtk::ListView::new(Some(selection.clone()), Some(factory));
    list.set_single_click_activate(true);
    list.connect_activate(|_, pos| println!("activate (recall): row {pos}"));

    let window = gtk::ApplicationWindow::builder()
        .application(app)
        .title("yankout spike: list + capture drag")
        .default_width(320)
        .default_height(360)
        .child(&list)
        .build();

    let drag = gtk::DragSource::new();
    drag.set_actions(gdk::DragAction::COPY);
    drag.set_propagation_phase(gtk::PropagationPhase::Capture);
    let sel = selection.clone();
    drag.connect_prepare(move |_, x, y| {
        let obj = sel.selected_item().and_downcast::<gtk::StringObject>()?;
        println!(
            "prepare: drag of selected row {} ({:?}), press at ({x:.0},{y:.0})",
            sel.selected(),
            obj.string()
        );
        Some(gdk::ContentProvider::for_value(&obj.string().to_value()))
    });
    drag.connect_drag_end(|_, _, _| println!("drag-end (drop landed)"));
    drag.connect_drag_cancel(|_, _, reason| {
        println!("drag-cancel: {reason:?}");
        false
    });
    window.add_controller(drag);

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
