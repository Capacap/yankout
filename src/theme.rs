//! Styling: a neutral default theme with one user layer above it — the
//! config-dir `style.css` if present, or an explicit `--css <file>` —
//! so any deployment can match its desktop without wrapper hacks.

use gtk4 as gtk;

use gtk::gdk;

const DEFAULT: &str = include_str!("theme.css");

/// Read the user layer up front so a bad file fails before GTK starts.
/// `--css` wins outright; otherwise the config-dir file applies when it
/// exists (absence is the common case and silent, any other error is not).
pub fn read_user_css(path: Option<&str>) -> Result<Option<String>, String> {
    if let Some(p) = path {
        return std::fs::read_to_string(p)
            .map(Some)
            .map_err(|e| format!("cannot read css file {p}: {e}"));
    }
    let Some(p) = config_css_path() else {
        return Ok(None);
    };
    match std::fs::read_to_string(&p) {
        Ok(css) => Ok(Some(css)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("cannot read css file {}: {e}", p.display())),
    }
}

/// `$XDG_CONFIG_HOME/yankout/style.css`, falling back to `~/.config`.
pub fn config_css_path() -> Option<std::path::PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|v| !v.is_empty())
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".config"))
        })?;
    Some(base.join("yankout").join("style.css"))
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
