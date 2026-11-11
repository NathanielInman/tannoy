//! The drawer: the cover, the track, a scrub bar, the controls, the
//! players to pick from and the output device. While do not disturb or a
//! screen share hides titles it says which and keeps only the controls.

use crate::anim::{Motion, Spring};
use crate::cover::Cover;
use crate::stage::Stage;
use bar_anchor::Anchor;
use gtk::prelude::*;
use gtk::{gdk, glib};
use gtk4_layer_shell::{Edge as LsEdge, KeyboardMode, Layer, LayerShell};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant};
use tannoy_core::audio::Sink;
use tannoy_core::config::Config;
use tannoy_core::model::{Player, Snapshot, Status};
use tannoy_core::protocol::{Event, Request};
use tannoy_core::text::clock;

pub enum Msg {
    Event(Event),
    /// A watched stylesheet or the config changed.
    Restyle,
    /// The default output device, read off the main thread.
    Sink(Option<Sink>),
}

const PLAY: &str = "\u{f040a}";
const PAUSE: &str = "\u{f03e4}";
const PREV: &str = "\u{f04ae}";
const NEXT: &str = "\u{f04ad}";
const MUSIC: &str = "\u{f075a}";
const SPEAKER: &str = "\u{f04c3}";
const SPEAKER_BT: &str = "\u{f00af}";

/// Glyphs for the podcast skip buttons, by how far they jump.
fn skip_glyphs(secs: u32) -> (String, String) {
    match secs {
        5 => ("\u{f11f9}".into(), "\u{f11f8}".into()),
        10 => ("\u{f0d2a}".into(), "\u{f0d71}".into()),
        15 => ("\u{f1946}".into(), "\u{f193a}".into()),
        30 => ("\u{f0d96}".into(), "\u{f0d06}".into()),
        n => (format!("\u{f045f} {n}"), format!("{n} \u{f0211}")),
    }
}

/// How long after a drag the scrub bar ignores the daemon's position, so
/// it does not jump back before the player catches up.
const SCRUB_HOLD: Duration = Duration::from_millis(1500);

fn label(text: &str, classes: &[&str]) -> gtk::Label {
    let l = gtk::Label::new(Some(text));
    l.set_xalign(0.0);
    for c in classes {
        l.add_css_class(c);
    }
    l
}

fn glyph_button(text: &str, tip: &str) -> gtk::Button {
    let b = gtk::Button::with_label(text);
    b.set_tooltip_text(Some(tip));
    b.set_focus_on_click(false);
    b
}

fn send_quiet(req: Request) {
    std::thread::spawn(move || {
        let _ = tannoy_core::protocol::request(&req);
    });
}

fn find_monitor(name: &str) -> Option<gdk::Monitor> {
    let display = gdk::Display::default()?;
    let list = display.monitors();
    (0..list.n_items())
        .filter_map(|i| list.item(i).and_downcast::<gdk::Monitor>())
        .find(|m| m.connector().is_some_and(|c| c == name))
}

fn set_state_class(w: &impl IsA<gtk::Widget>, state: &str) {
    for c in ["playing", "paused", "hidden", "idle"] {
        w.remove_css_class(c);
    }
    w.add_css_class(state);
}

/// Reads the default sink with wpctl. Runs on a helper thread.
fn read_sink() -> Option<Sink> {
    let run = |args: &[&str]| {
        std::process::Command::new("wpctl")
            .args(args)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
    };
    let mut sink = tannoy_core::audio::parse_inspect(&run(&["inspect", "@DEFAULT_AUDIO_SINK@"])?);
    if let Some((v, muted)) = run(&["get-volume", "@DEFAULT_AUDIO_SINK@"])
        .as_deref()
        .and_then(tannoy_core::audio::parse_volume)
    {
        sink.volume = Some(v);
        sink.muted = muted;
    }
    (!sink.name.is_empty()).then_some(sink)
}

/// What a button does.
type Action = Box<dyn Fn(&Rc<Drawer>)>;

pub struct Drawer {
    window: gtk::ApplicationWindow,
    tx: async_channel::Sender<Msg>,
    stage: Stage,
    spring: Spring,
    visible: Cell<bool>,
    anchor: RefCell<Option<Anchor>>,
    snap: RefCell<Option<Snapshot>>,
    cfg: RefCell<Config>,

    lamp: gtk::Label,
    sub: gtk::Label,
    chip: gtk::Label,
    notice: gtk::Label,

    now: gtk::Stack,
    art_slot: gtk::Stack,
    art: Cover,
    art_path: RefCell<Option<String>>,
    kind: gtk::Label,
    title: gtk::Label,
    artist: gtk::Label,
    album: gtk::Label,
    scale: gtk::Scale,
    pos: gtk::Label,
    len: gtk::Label,
    /// When the user last moved the scrub bar.
    scrubbed: Cell<Option<Instant>>,
    /// A seek waiting for the drag to settle.
    seek_to: Cell<Option<f64>>,
    seek_timer: RefCell<Option<glib::SourceId>>,
    /// Set while the scrub bar is updated from the daemon, so that update
    /// is not taken for a drag.
    updating: Cell<bool>,

    controls: gtk::Box,
    back: gtk::Button,
    prev: gtk::Button,
    play: gtk::Button,
    next: gtk::Button,
    fwd: gtk::Button,

    players_label: gtk::Label,
    players_aside: gtk::Label,
    players: gtk::Box,
    players_sig: RefCell<String>,

    out_card: gtk::Box,
    out_icon: gtk::Label,
    out_name: gtk::Label,
    out_btn: gtk::Button,
    /// Why the hidden card shows: do not disturb or a screen share.
    hidden_why: gtk::Label,
}

impl Drawer {
    pub fn new(app: &gtk::Application, tx: async_channel::Sender<Msg>) -> Rc<Drawer> {
        let (cfg, _) = Config::load();
        let window = gtk::ApplicationWindow::new(app);
        window.add_css_class("tannoy");
        window.set_decorated(false);
        window.set_title(Some("Tannoy"));
        window.init_layer_shell();
        window.set_layer(Layer::Overlay);
        window.set_namespace(Some("tannoy"));
        for e in [LsEdge::Top, LsEdge::Bottom, LsEdge::Left, LsEdge::Right] {
            window.set_anchor(e, true);
        }
        window.set_exclusive_zone(0);
        window.set_keyboard_mode(KeyboardMode::Exclusive);

        // ---- header
        let lamp = label("\u{25cf}", &["lamp", "idle"]);
        lamp.set_valign(gtk::Align::Center);
        let brand = label("TANNOY", &["brand"]);
        let sub = label("", &["sub"]);
        sub.set_ellipsize(gtk::pango::EllipsizeMode::End);
        let who = gtk::Box::new(gtk::Orientation::Vertical, 0);
        who.append(&brand);
        who.append(&sub);
        who.set_hexpand(true);
        let chip = label("idle", &["chip", "idle"]);
        chip.set_valign(gtk::Align::Center);
        let header = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        header.append(&lamp);
        header.append(&who);
        header.append(&chip);

        let notice = label("", &["notice"]);
        notice.set_wrap(true);
        notice.set_visible(false);

        // ---- the track
        let art = Cover::new(112, 8.0);
        art.add_css_class("art");
        let none = label(MUSIC, &["art-none"]);
        none.set_xalign(0.5);
        none.set_size_request(112, 112);
        let art_slot = gtk::Stack::new();
        art_slot.add_named(&art, Some("art"));
        art_slot.add_named(&none, Some("none"));
        art_slot.set_valign(gtk::Align::Start);
        art_slot.set_hhomogeneous(true);
        art_slot.set_vhomogeneous(true);

        let kind = label("", &["kind"]);
        let title = label("", &["title"]);
        title.set_wrap(true);
        title.set_wrap_mode(gtk::pango::WrapMode::WordChar);
        title.set_lines(3);
        title.set_ellipsize(gtk::pango::EllipsizeMode::End);
        let artist = label("", &["artist"]);
        artist.set_ellipsize(gtk::pango::EllipsizeMode::End);
        let album = label("", &["album"]);
        album.set_ellipsize(gtk::pango::EllipsizeMode::End);
        let info = gtk::Box::new(gtk::Orientation::Vertical, 3);
        info.set_hexpand(true);
        info.set_valign(gtk::Align::Center);
        info.append(&kind);
        info.append(&title);
        info.append(&artist);
        info.append(&album);
        let top = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        top.append(&art_slot);
        top.append(&info);

        let scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 1.0, 1.0);
        scale.set_draw_value(false);
        scale.add_css_class("scrub");
        scale.set_hexpand(true);
        let pos = label("0:00", &["time"]);
        let len = label("", &["time"]);
        len.set_hexpand(true);
        len.set_xalign(1.0);
        let times = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        times.append(&pos);
        times.append(&len);
        let track = gtk::Box::new(gtk::Orientation::Vertical, 4);
        track.append(&top);
        track.append(&scale);
        track.append(&times);

        // ---- hidden by do not disturb or a screen share
        let hidden = gtk::Box::new(gtk::Orientation::Vertical, 6);
        hidden.add_css_class("card");
        hidden.add_css_class("hidden");
        let hrow = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let hl = label("HIDDEN", &["t-label", "head"]);
        hl.set_hexpand(true);
        hrow.append(&hl);
        let hidden_why = label("DO NOT DISTURB IS ON", &["t-label"]);
        hrow.append(&hidden_why);
        hidden.append(&hrow);
        let htext = label(
            "The title and cover stay off the bar while do not disturb is on or the screen is shared. The controls still work.",
            &[],
        );
        htext.set_wrap(true);
        hidden.append(&htext);

        // ---- nothing playing
        let empty = gtk::Box::new(gtk::Orientation::Vertical, 6);
        empty.add_css_class("card");
        let et = label(
            "Nothing is playing. Spotify, a browser tab or any MPRIS player shows up here as soon as it starts.",
            &[],
        );
        et.set_wrap(true);
        empty.append(&et);

        let now = gtk::Stack::new();
        now.set_vhomogeneous(false);
        now.set_interpolate_size(true);
        now.set_transition_type(gtk::StackTransitionType::Crossfade);
        now.set_transition_duration(160);
        now.add_named(&track, Some("track"));
        now.add_named(&hidden, Some("hidden"));
        now.add_named(&empty, Some("empty"));

        // ---- controls
        let (b15, f15) = skip_glyphs(cfg.drawer.skip);
        let back = glyph_button(&b15, &format!("Back {} seconds", cfg.drawer.skip));
        back.add_css_class("skip");
        let prev = glyph_button(PREV, "Previous");
        let play = glyph_button(PLAY, "Play or pause");
        play.add_css_class("primary");
        let next = glyph_button(NEXT, "Next");
        let fwd = glyph_button(&f15, &format!("Forward {} seconds", cfg.drawer.skip));
        fwd.add_css_class("skip");
        let controls = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        controls.add_css_class("controls");
        controls.set_halign(gtk::Align::Center);
        for b in [&back, &prev, &play, &next, &fwd] {
            controls.append(b);
        }

        // ---- players
        let prow = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let players_label = label("PLAYERS", &["t-label"]);
        players_label.set_hexpand(true);
        let players_aside = label("", &["t-label"]);
        prow.append(&players_label);
        prow.append(&players_aside);
        let players = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        players.add_css_class("players");
        let pbox = gtk::Box::new(gtk::Orientation::Vertical, 6);
        pbox.append(&prow);
        pbox.append(&players);

        // ---- output
        let out_card = gtk::Box::new(gtk::Orientation::Vertical, 6);
        out_card.add_css_class("card");
        out_card.append(&label("OUTPUT", &["t-label"]));
        let orow = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let out_icon = label(SPEAKER, &["output-icon"]);
        let out_name = label("", &[]);
        out_name.set_hexpand(true);
        out_name.set_ellipsize(gtk::pango::EllipsizeMode::End);
        let out_btn = gtk::Button::with_label("Change");
        out_btn.add_css_class("change");
        out_btn.set_focus_on_click(false);
        out_btn.set_valign(gtk::Align::Center);
        orow.append(&out_icon);
        orow.append(&out_name);
        orow.append(&out_btn);
        out_card.append(&orow);

        let foot = label(
            "Space play/pause \u{b7} \u{2190}/\u{2192} seek \u{b7} Esc close",
            &["foot"],
        );

        let panel = gtk::Box::new(gtk::Orientation::Vertical, 12);
        panel.add_css_class("panel");
        panel.append(&header);
        panel.append(&notice);
        panel.append(&now);
        panel.append(&controls);
        panel.append(&pbox);
        panel.append(&out_card);
        panel.append(&foot);

        let spring = Spring::new(&panel, Motion::PANEL);
        let stage = Stage::new(
            &spring,
            cfg.drawer.width,
            cfg.drawer.max_height,
            cfg.drawer.gap,
        );
        window.set_child(Some(&stage));

        let d = Rc::new(Drawer {
            window,
            tx,
            stage,
            spring,
            visible: Cell::new(false),
            anchor: RefCell::new(None),
            snap: RefCell::new(None),
            cfg: RefCell::new(cfg),
            lamp,
            sub,
            chip,
            notice,
            now,
            art_slot,
            art,
            art_path: RefCell::new(None),
            kind,
            title,
            artist,
            album,
            scale,
            pos,
            len,
            scrubbed: Cell::new(None),
            seek_to: Cell::new(None),
            seek_timer: RefCell::new(None),
            updating: Cell::new(false),
            controls,
            back,
            prev,
            play,
            next,
            fwd,
            players_label,
            players_aside,
            players,
            players_sig: RefCell::new(String::new()),
            out_card,
            out_icon,
            out_name,
            out_btn,
            hidden_why,
        });
        d.connect();
        d
    }

    fn connect(self: &Rc<Self>) {
        {
            let w = Rc::downgrade(self);
            self.stage.connect_outside(move || {
                if let Some(d) = w.upgrade() {
                    d.hide();
                }
            });
            let s = self.spring.clone();
            self.stage.connect_placed(move |(x, y)| s.set_origin(x, y));
        }
        let skip = |d: &Rc<Drawer>, sign: f64| {
            let secs = d.cfg.borrow().drawer.skip as f64;
            send_quiet(Request::Seek {
                offset: sign * secs,
            });
        };
        let bind = |b: &gtk::Button, f: Action| {
            let w = Rc::downgrade(self);
            b.connect_clicked(move |_| {
                if let Some(d) = w.upgrade() {
                    f(&d);
                }
            });
        };
        bind(&self.back, Box::new(move |d| skip(d, -1.0)));
        bind(&self.fwd, Box::new(move |d| skip(d, 1.0)));
        bind(&self.prev, Box::new(|_| send_quiet(Request::Previous)));
        bind(&self.next, Box::new(|_| send_quiet(Request::Next)));
        bind(&self.play, Box::new(|_| send_quiet(Request::PlayPause)));
        bind(&self.out_btn, Box::new(|d| d.change_output()));
        {
            let w = Rc::downgrade(self);
            self.scale.connect_change_value(move |_, _, value| {
                if let Some(d) = w.upgrade() {
                    if !d.updating.get() {
                        d.scrub(value);
                    }
                }
                glib::Propagation::Proceed
            });
        }
        {
            let keys = gtk::EventControllerKey::new();
            let w = Rc::downgrade(self);
            keys.connect_key_pressed(move |_, key, _, _| {
                let Some(d) = w.upgrade() else {
                    return glib::Propagation::Proceed;
                };
                d.key(key)
            });
            self.window.add_controller(keys);
        }
        // The position moves between the daemon's readings.
        let w = Rc::downgrade(self);
        glib::timeout_add_local(Duration::from_millis(250), move || {
            let Some(d) = w.upgrade() else {
                return glib::ControlFlow::Break;
            };
            if d.visible.get() {
                d.tick();
            }
            glib::ControlFlow::Continue
        });
    }

    pub fn handle(self: &Rc<Self>, msg: Msg) {
        match msg {
            Msg::Event(Event::Snapshot { snapshot }) => {
                *self.snap.borrow_mut() = Some(*snapshot);
                if self.visible.get() {
                    self.render();
                }
            }
            Msg::Event(Event::Show { anchor }) => self.show(anchor),
            Msg::Event(Event::Toggle { anchor }) => {
                if self.visible.get() {
                    self.hide();
                } else {
                    self.show(anchor);
                }
            }
            Msg::Event(Event::Hide) => self.hide(),
            Msg::Restyle => {
                crate::linux::apply_styles();
                let (cfg, _) = Config::load();
                self.stage
                    .set_geometry(cfg.drawer.width, cfg.drawer.max_height, cfg.drawer.gap);
                let (b, f) = skip_glyphs(cfg.drawer.skip);
                self.back.set_label(&b);
                self.fwd.set_label(&f);
                *self.cfg.borrow_mut() = cfg;
                if self.visible.get() {
                    self.render();
                }
            }
            Msg::Sink(sink) => self.render_sink(sink),
        }
    }

    fn key(self: &Rc<Self>, key: gdk::Key) -> glib::Propagation {
        match key {
            gdk::Key::Escape => self.hide(),
            gdk::Key::space | gdk::Key::k => send_quiet(Request::PlayPause),
            gdk::Key::Left | gdk::Key::j => send_quiet(Request::Seek { offset: -5.0 }),
            gdk::Key::Right | gdk::Key::l => send_quiet(Request::Seek { offset: 5.0 }),
            gdk::Key::n => send_quiet(Request::Next),
            gdk::Key::p => send_quiet(Request::Previous),
            _ => return glib::Propagation::Proceed,
        }
        glib::Propagation::Stop
    }

    fn show(self: &Rc<Self>, anchor: Option<Anchor>) {
        if let Some(name) = anchor.as_ref().and_then(|a| a.monitor.clone()) {
            if let Some(m) = find_monitor(&name) {
                self.window.set_monitor(Some(&m));
            }
        }
        *self.anchor.borrow_mut() = anchor.clone();
        self.stage.set_anchor(anchor);
        let was_visible = self.visible.replace(true);
        self.render();
        self.window.set_visible(true);
        self.window.present();
        if !was_visible {
            self.spring.play_in(0);
            send_quiet(Request::BoardVisible { visible: true });
            self.refresh_sink();
        }
    }

    fn hide(self: &Rc<Self>) {
        if !self.visible.replace(false) {
            return;
        }
        let w = Rc::downgrade(self);
        self.spring.play_out(move || {
            if let Some(d) = w.upgrade() {
                if !d.visible.get() {
                    d.window.set_visible(false);
                }
            }
        });
        send_quiet(Request::BoardVisible { visible: false });
    }

    fn refresh_sink(&self) {
        if self.cfg.borrow().output_command().is_none() && !self.out_card.is_visible() {
            return;
        }
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let _ = tx.send_blocking(Msg::Sink(read_sink()));
        });
    }

    /// Runs the output command with the drawer closed (it usually opens a
    /// picker that needs the keyboard), then reads the device again.
    fn change_output(self: &Rc<Self>) {
        let Some(cmd) = self.cfg.borrow().output_command() else {
            return;
        };
        self.hide();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let _ = std::process::Command::new("sh")
                .arg("-c")
                .arg(&cmd)
                .stdin(std::process::Stdio::null())
                .status();
            let _ = tx.send_blocking(Msg::Sink(read_sink()));
        });
    }

    fn render_sink(&self, sink: Option<Sink>) {
        let has_cmd = self.cfg.borrow().output_command().is_some();
        self.out_btn.set_visible(has_cmd);
        match sink {
            Some(s) => {
                self.out_card.set_visible(true);
                self.out_icon
                    .set_label(if s.bluetooth { SPEAKER_BT } else { SPEAKER });
                let mut text = s.name.clone();
                if let Some(v) = s.volume {
                    text.push_str(&format!(" \u{b7} {:.0}%", v * 100.0));
                }
                if s.muted {
                    text.push_str(" \u{b7} muted");
                }
                self.out_name.set_label(&text);
            }
            None => self.out_card.set_visible(has_cmd),
        }
    }

    /// The user dragged or clicked the scrub bar: seek once it settles.
    fn scrub(self: &Rc<Self>, value: f64) {
        self.scrubbed.set(Some(Instant::now()));
        self.seek_to.set(Some(value));
        self.pos.set_label(&clock((value.max(0.0) * 1e6) as u64));
        if let Some(id) = self.seek_timer.borrow_mut().take() {
            id.remove();
        }
        let w = Rc::downgrade(self);
        let id = glib::timeout_add_local_once(Duration::from_millis(120), move || {
            let Some(d) = w.upgrade() else { return };
            d.seek_timer.borrow_mut().take();
            if let Some(v) = d.seek_to.take() {
                send_quiet(Request::SetPosition {
                    position: v.max(0.0),
                });
            }
        });
        *self.seek_timer.borrow_mut() = Some(id);
    }

    fn active(&self) -> Option<Player> {
        self.snap.borrow().as_ref()?.active_player().cloned()
    }

    /// Moves the scrub bar and the clock between the daemon's readings.
    fn tick(&self) {
        let Some(p) = self.active() else { return };
        if self
            .scrubbed
            .get()
            .is_some_and(|t| t.elapsed() < SCRUB_HOLD)
        {
            return;
        }
        let now = tannoy_core::now();
        let at = p.position_now(now);
        self.pos.set_label(&clock(at));
        if let Some(len) = p.track.length {
            self.updating.set(true);
            self.scale.set_range(0.0, (len as f64 / 1e6).max(1.0));
            self.scale.set_value(at as f64 / 1e6);
            self.updating.set(false);
        }
    }

    fn render(&self) {
        let snap = self.snap.borrow();
        let Some(s) = snap.as_ref() else {
            self.sub.set_label("waiting for the daemon");
            self.now.set_visible_child_name("empty");
            self.controls.set_visible(false);
            return;
        };
        let cfg = self.cfg.borrow();
        match &s.config_error {
            Some(e) => {
                self.notice
                    .set_label(&format!("Config problem, using defaults: {e}"));
                self.notice.set_visible(true);
            }
            None => self.notice.set_visible(false),
        }

        let active = s
            .active_player()
            .filter(|p| p.status != Status::Stopped || !p.track.is_empty());
        let n = s.players.len();
        let count = match n {
            0 => "no players".to_string(),
            1 => "1 player".to_string(),
            n => format!("{n} players"),
        };
        self.sub.set_label(&match active {
            Some(p) => format!("{} \u{b7} {count}", p.name),
            None => count,
        });

        let state = match active {
            None => "idle",
            Some(_) if s.hidden => "hidden",
            Some(p) => match p.status {
                Status::Playing => "playing",
                Status::Paused => "paused",
                Status::Stopped => "idle",
            },
        };
        set_state_class(&self.lamp, state);
        set_state_class(&self.chip, state);
        self.chip.set_label(match state {
            "playing" => "playing",
            "paused" => "paused",
            "hidden" => "hidden",
            _ => "idle",
        });

        match active {
            None => {
                self.now.set_visible_child_name("empty");
                self.controls.set_visible(false);
            }
            Some(p) => {
                self.controls.set_visible(true);
                if s.hidden {
                    self.hidden_why
                        .set_label(&s.hidden_reason(&cfg).to_uppercase());
                    self.now.set_visible_child_name("hidden");
                } else {
                    self.now.set_visible_child_name("track");
                    self.render_track(p, s);
                }
                self.render_controls(p, &cfg);
            }
        }
        self.render_players(s);
        drop(cfg);
        drop(snap);
        self.tick();
    }

    fn render_track(&self, p: &Player, s: &Snapshot) {
        let t = &p.track;
        let player = p.name.to_uppercase();
        self.kind.set_label(&if t.is_podcast() {
            format!("PODCAST \u{b7} {player}")
        } else {
            player
        });
        self.title.set_label(if t.title.is_empty() {
            "Untitled"
        } else {
            &t.title
        });
        let artist = t.artist();
        self.artist.set_label(&artist);
        self.artist.set_visible(!artist.is_empty());
        self.album.set_label(&t.album);
        self.album.set_visible(!t.album.is_empty());
        self.len.set_label(&t.length.map(clock).unwrap_or_default());
        self.scale.set_sensitive(p.can_seek && t.length.is_some());

        if *self.art_path.borrow() != s.art {
            let shown = self.art.set_path(s.art.as_deref());
            self.art_slot
                .set_visible_child_name(if shown { "art" } else { "none" });
            *self.art_path.borrow_mut() = s.art.clone();
        }
    }

    fn render_controls(&self, p: &Player, cfg: &Config) {
        self.play.set_label(if p.status == Status::Playing {
            PAUSE
        } else {
            PLAY
        });
        self.play.set_sensitive(p.can_control);
        self.prev.set_sensitive(p.can_go_previous);
        self.next.set_sensitive(p.can_go_next);
        let skips = match cfg.drawer.skips.as_str() {
            "always" => true,
            "never" => false,
            _ => p.track.is_podcast(),
        };
        for b in [&self.back, &self.fwd] {
            b.set_visible(skips);
            b.set_sensitive(p.can_seek);
        }
    }

    fn render_players(&self, s: &Snapshot) {
        let sig = format!(
            "{:?}|{:?}|{}",
            s.active,
            s.picked,
            s.players
                .iter()
                .map(|p| format!("{}:{}:{}", p.bus, p.name, p.status.as_str()))
                .collect::<Vec<_>>()
                .join(",")
        );
        self.players_aside.set_label(match s.picked {
            Some(_) => "PICKED \u{b7} MEDIA KEYS FOLLOW IT",
            None => "MEDIA KEYS FOLLOW THE LATEST",
        });
        self.players_label.set_visible(!s.players.is_empty());
        self.players_aside.set_visible(!s.players.is_empty());
        if *self.players_sig.borrow() == sig {
            return;
        }
        *self.players_sig.borrow_mut() = sig;
        while let Some(c) = self.players.first_child() {
            self.players.remove(&c);
        }
        let chip = |text: &str, dot: Option<&str>, on: bool, req: Request, tip: &str| {
            let row = gtk::Box::new(gtk::Orientation::Horizontal, 5);
            if let Some(state) = dot {
                row.append(&label("\u{25cf}", &["dot", state]));
            }
            row.append(&label(text, &[]));
            let b = gtk::Button::new();
            b.set_child(Some(&row));
            b.set_focus_on_click(false);
            b.set_tooltip_text(Some(tip));
            if on {
                b.add_css_class("on");
            }
            b.connect_clicked(move |_| send_quiet(req.clone()));
            b
        };
        for p in &s.players {
            let on = s.active.as_deref() == Some(p.bus.as_str());
            let b = chip(
                &p.name,
                Some(p.status.as_str()),
                on,
                Request::Pick {
                    player: Some(p.key.clone()),
                },
                &format!("Send the media keys to {}", p.name),
            );
            self.players.append(&b);
        }
        if s.picked.is_some() {
            let b = chip(
                "Auto",
                None,
                false,
                Request::Pick { player: None },
                "Follow whichever player started playing last",
            );
            self.players.append(&b);
        }
    }
}
