//! Native clipboard watcher: speaks ext-data-control-v1 (or the older
//! zwlr-data-control) directly and feeds the store. No window, no GTK —
//! the data-control device delivers a `selection` event on every
//! clipboard change, and content is read through a pipe.
//!
//! One MIME type is stored per selection, chosen by [`crate::mime::pick`];
//! offers advertising the password-manager hint are skipped entirely.

use std::collections::HashMap;
use std::os::fd::AsFd;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use rustix::event::{PollFd, PollFlags};

use wayland_client::protocol::{wl_registry, wl_seat::WlSeat};
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle, backend::ObjectId, event_created_child};
use wayland_protocols::ext::data_control::v1::client::{
    ext_data_control_device_v1::{self, ExtDataControlDeviceV1},
    ext_data_control_manager_v1::ExtDataControlManagerV1,
    ext_data_control_offer_v1::{self, ExtDataControlOfferV1},
};
use wayland_protocols_wlr::data_control::v1::client::{
    zwlr_data_control_device_v1::{self, ZwlrDataControlDeviceV1},
    zwlr_data_control_manager_v1::ZwlrDataControlManagerV1,
    zwlr_data_control_offer_v1::{self, ZwlrDataControlOfferV1},
};

use crate::Error;
use crate::store::Writer;

/// Sanity cap on a single entry; a source offering more than this is
/// broken or hostile, and 750 such entries would fill a disk.
const MAX_ENTRY_BYTES: usize = 64 * 1024 * 1024;

/// A source that stops writing for this long is treated as hung and its
/// selection skipped; the watcher runs on one thread, and a stall here
/// would drop every later copy while the lock still says it is alive.
const SOURCE_STALL: Duration = Duration::from_secs(5);

pub fn run(dir: PathBuf, cap: usize) -> Result<(), Error> {
    let writer = Writer::open(dir, cap)?;
    let conn = Connection::connect_to_env()
        .map_err(|e| Error(format!("connecting to wayland: {e}")))?;
    let mut queue = conn.new_event_queue();
    let qh = queue.handle();
    conn.display().get_registry(&qh, ());

    let mut state = State {
        seat: None,
        ext_mgr: None,
        wlr_mgr: None,
        mimes: HashMap::new(),
        pending: None,
        finished: false,
        writer,
    };
    queue
        .roundtrip(&mut state)
        .map_err(|e| Error(format!("wayland registry roundtrip: {e}")))?;

    let seat = state
        .seat
        .clone()
        .ok_or_else(|| Error("compositor advertised no wl_seat".into()))?;
    if let Some(mgr) = &state.ext_mgr {
        eprintln!("yankout watch: using ext-data-control-v1");
        mgr.get_data_device(&seat, &qh, ());
    } else if let Some(mgr) = &state.wlr_mgr {
        eprintln!("yankout watch: using zwlr-data-control (ext not offered)");
        mgr.get_data_device(&seat, &qh, ());
    } else {
        return Err(Error(
            "compositor offers neither ext-data-control-v1 nor zwlr-data-control; \
             use cliphist's wl-paste watchers instead"
                .into(),
        ));
    }

    loop {
        queue
            .blocking_dispatch(&mut state)
            .map_err(|e| Error(format!("wayland dispatch: {e}")))?;
        if state.finished {
            return Err(Error("compositor closed the data-control device".into()));
        }
        if let Some(offer) = state.pending.take() {
            let mimes = state.mimes.remove(&offer.id()).unwrap_or_default();
            match read_selection(&conn, &offer, &mimes) {
                Ok(Some(content)) => {
                    if let Err(e) = state.writer.store(&content) {
                        eprintln!("yankout watch: {e}");
                    }
                }
                Ok(None) => {}
                Err(e) => eprintln!("yankout watch: {e}"),
            }
            offer.destroy();
        }
    }
}

/// The two protocols are request-for-request identical; this evens out
/// the types so everything past dispatch is written once.
enum Offer {
    Ext(ExtDataControlOfferV1),
    Wlr(ZwlrDataControlOfferV1),
}

impl Offer {
    fn id(&self) -> ObjectId {
        match self {
            Offer::Ext(o) => o.id(),
            Offer::Wlr(o) => o.id(),
        }
    }

    fn receive(&self, mime: String, fd: std::os::fd::BorrowedFd) {
        match self {
            Offer::Ext(o) => o.receive(mime, fd),
            Offer::Wlr(o) => o.receive(mime, fd),
        }
    }

    fn destroy(&self) {
        match self {
            Offer::Ext(o) => o.destroy(),
            Offer::Wlr(o) => o.destroy(),
        }
    }
}

struct State {
    seat: Option<WlSeat>,
    ext_mgr: Option<ExtDataControlManagerV1>,
    wlr_mgr: Option<ZwlrDataControlManagerV1>,
    /// MIME types announced so far, per not-yet-claimed offer.
    mimes: HashMap<ObjectId, Vec<String>>,
    /// Selection to ingest once the current dispatch batch is done —
    /// content is read outside the event callbacks.
    pending: Option<Offer>,
    finished: bool,
    writer: Writer,
}

impl State {
    fn on_data_offer(&mut self, id: ObjectId) {
        self.mimes.insert(id, Vec::new());
    }

    fn on_mime(&mut self, id: ObjectId, mime: String) {
        if let Some(list) = self.mimes.get_mut(&id) {
            list.push(mime);
        }
    }

    fn on_selection(&mut self, offer: Option<Offer>) {
        // several selections in one batch: earlier ones are already stale
        if let Some(old) = self.pending.take() {
            self.mimes.remove(&old.id());
            old.destroy();
        }
        self.pending = offer;
    }

    /// The primary selection (mouse highlight) is deliberately not
    /// history; its offers still have to be destroyed.
    fn on_primary(&mut self, offer: Option<Offer>) {
        if let Some(o) = offer {
            self.mimes.remove(&o.id());
            o.destroy();
        }
    }
}

fn read_selection(conn: &Connection, offer: &Offer, mimes: &[String]) -> Result<Option<Vec<u8>>, Error> {
    let Some(mime) = crate::mime::pick(mimes.iter().map(String::as_str)) else {
        return Ok(None);
    };
    let (read_end, write_end) =
        rustix::pipe::pipe().map_err(|e| Error(format!("creating pipe: {e}")))?;
    offer.receive(mime.to_string(), write_end.as_fd());
    // the request holds its own dup of the fd; ours must close or the
    // read below never sees EOF
    drop(write_end);
    conn.flush()
        .map_err(|e| Error(format!("flushing receive request: {e}")))?;

    let content = read_pipe(read_end, SOURCE_STALL)?;
    if worth_storing(mime, &content) {
        Ok(Some(content))
    } else {
        Ok(None)
    }
}

/// Drain a pipe to EOF, giving up if the writer goes quiet for `stall`.
fn read_pipe(fd: impl AsFd, stall: Duration) -> Result<Vec<u8>, Error> {
    let mut content = Vec::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let deadline = Instant::now() + stall;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err(Error(format!(
                    "source stalled for {:?}, selection skipped",
                    stall
                )));
            }
            let mut fds = [PollFd::new(&fd, PollFlags::IN)];
            let timeout = rustix::event::Timespec::try_from(left)
                .map_err(|_| Error("poll timeout out of range".into()))?;
            match rustix::event::poll(&mut fds, Some(&timeout)) {
                Ok(0) => continue,
                Ok(_) => break,
                Err(rustix::io::Errno::INTR) => continue,
                Err(e) => return Err(Error(format!("waiting for selection: {e}"))),
            }
        }
        match rustix::io::read(&fd, &mut buf) {
            Ok(0) => return Ok(content),
            Ok(n) => {
                content.extend_from_slice(&buf[..n]);
                if content.len() > MAX_ENTRY_BYTES {
                    return Err(Error(format!("selection exceeds {MAX_ENTRY_BYTES} bytes, skipped")));
                }
            }
            Err(rustix::io::Errno::INTR) => continue,
            Err(e) => return Err(Error(format!("reading selection: {e}"))),
        }
    }
}

/// Empty selections and whitespace-only text are noise, not history.
fn worth_storing(mime: &str, content: &[u8]) -> bool {
    if content.is_empty() {
        return false;
    }
    if mime.starts_with("image/") {
        return true;
    }
    !content.iter().all(|b| b.is_ascii_whitespace())
}

impl Dispatch<wl_registry::WlRegistry, ()> for State {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global { name, interface, .. } = event {
            match interface.as_str() {
                "wl_seat" if state.seat.is_none() => {
                    state.seat = Some(registry.bind(name, 1, qh, ()));
                }
                "ext_data_control_manager_v1" => {
                    state.ext_mgr = Some(registry.bind(name, 1, qh, ()));
                }
                "zwlr_data_control_manager_v1" => {
                    state.wlr_mgr = Some(registry.bind(name, 1, qh, ()));
                }
                _ => {}
            }
        }
    }
}

impl Dispatch<WlSeat, ()> for State {
    fn event(
        _: &mut Self,
        _: &WlSeat,
        _: <WlSeat as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ExtDataControlManagerV1, ()> for State {
    fn event(
        _: &mut Self,
        _: &ExtDataControlManagerV1,
        _: <ExtDataControlManagerV1 as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwlrDataControlManagerV1, ()> for State {
    fn event(
        _: &mut Self,
        _: &ZwlrDataControlManagerV1,
        _: <ZwlrDataControlManagerV1 as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ExtDataControlDeviceV1, ()> for State {
    fn event(
        state: &mut Self,
        _: &ExtDataControlDeviceV1,
        event: ext_data_control_device_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        use ext_data_control_device_v1::Event;
        match event {
            Event::DataOffer { id } => state.on_data_offer(id.id()),
            Event::Selection { id } => state.on_selection(id.map(Offer::Ext)),
            Event::PrimarySelection { id } => state.on_primary(id.map(Offer::Ext)),
            Event::Finished => state.finished = true,
            _ => {}
        }
    }

    event_created_child!(State, ExtDataControlDeviceV1, [
        ext_data_control_device_v1::EVT_DATA_OFFER_OPCODE => (ExtDataControlOfferV1, ()),
    ]);
}

impl Dispatch<ZwlrDataControlDeviceV1, ()> for State {
    fn event(
        state: &mut Self,
        _: &ZwlrDataControlDeviceV1,
        event: zwlr_data_control_device_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        use zwlr_data_control_device_v1::Event;
        match event {
            Event::DataOffer { id } => state.on_data_offer(id.id()),
            Event::Selection { id } => state.on_selection(id.map(Offer::Wlr)),
            Event::PrimarySelection { id } => state.on_primary(id.map(Offer::Wlr)),
            Event::Finished => state.finished = true,
            _ => {}
        }
    }

    event_created_child!(State, ZwlrDataControlDeviceV1, [
        zwlr_data_control_device_v1::EVT_DATA_OFFER_OPCODE => (ZwlrDataControlOfferV1, ()),
    ]);
}

impl Dispatch<ExtDataControlOfferV1, ()> for State {
    fn event(
        state: &mut Self,
        offer: &ExtDataControlOfferV1,
        event: ext_data_control_offer_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let ext_data_control_offer_v1::Event::Offer { mime_type } = event {
            state.on_mime(offer.id(), mime_type);
        }
    }
}

impl Dispatch<ZwlrDataControlOfferV1, ()> for State {
    fn event(
        state: &mut Self,
        offer: &ZwlrDataControlOfferV1,
        event: zwlr_data_control_offer_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let zwlr_data_control_offer_v1::Event::Offer { mime_type } = event {
            state.on_mime(offer.id(), mime_type);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_pipe_drains_to_eof() {
        let (r, w) = rustix::pipe::pipe().unwrap();
        rustix::io::write(&w, b"hello").unwrap();
        drop(w);
        assert_eq!(read_pipe(r, Duration::from_secs(1)).unwrap(), b"hello");
    }

    #[test]
    fn read_pipe_gives_up_on_a_silent_writer() {
        let (r, w) = rustix::pipe::pipe().unwrap();
        rustix::io::write(&w, b"partial").unwrap();
        let err = read_pipe(r, Duration::from_millis(50)).unwrap_err();
        assert!(err.0.contains("stalled"), "{err}");
        drop(w);
    }

    #[test]
    fn whitespace_only_text_is_not_stored() {
        assert!(!worth_storing("text/plain", b"  \n\t"));
        assert!(!worth_storing("text/plain", b""));
        assert!(worth_storing("text/plain", b" x "));
        // bytes that happen to look like whitespace are still an image
        assert!(worth_storing("image/png", b"\n\n"));
    }
}
