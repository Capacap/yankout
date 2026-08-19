//! Styling: a neutral default theme with a `--css <file>` layer above it,
//! so any deployment can match its desktop without wrapper hacks.

use gtk4 as gtk;

use gtk::gdk;

const DEFAULT: &str = include_str!("theme.css");

/// Read the `--css` file up front so a bad path fails before GTK starts.
pub fn read_user_css(path: Option<&str>) -> Result<Option<String>, String> {
    match path {
        None => Ok(None),
        Some(p) => std::fs::read_to_string(p)
            .map(Some)
            .map_err(|e| format!("cannot read css file {p}: {e}")),
    }
}

/// Install the default theme, then the user's css one priority step above
/// it. Needs a display, so call from `activate`, not before `run`.
pub fn apply(user_css: Option<&str>) {
    let display = gdk::Display::default().expect("gtk is running, a display exists");
    let base = gtk::CssProvider::new();
    base.load_from_string(DEFAULT);
    gtk::style_context_add_provider_for_display(
        &display,
        &base,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
    if let Some(css) = user_css {
        let user = gtk::CssProvider::new();
        user.load_from_string(css);
        gtk::style_context_add_provider_for_display(
            &display,
            &user,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION + 1,
        );
    }
}
