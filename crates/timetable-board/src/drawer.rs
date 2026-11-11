//! The drawer: the date and the state, three zone tiles, the next (or
//! current) meeting with Join and Copy link, the day strip with its free
//! gaps and a now line, the rest of today, and the focus card.

use crate::anim::{Motion, Spring};
use crate::stage::Stage;
use bar_anchor::Anchor;
use chrono::{DateTime, Utc};
use chrono_tz::Tz;
use gtk::prelude::*;
use gtk::{gdk, glib};
use gtk4_layer_shell::{Edge as LsEdge, KeyboardMode, Layer, LayerShell};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;
use timetable_core::agenda::{self, State};
use timetable_core::config::Config;
use timetable_core::expand::local_date;
use timetable_core::links;
use timetable_core::model::{Event, Snapshot, Source};
use timetable_core::protocol::{Event as BoardEvent, FocusAction, Request};
use timetable_core::text::{ago, countdown, minutes};

pub enum Msg {
    Event(BoardEvent),
    /// A watched stylesheet or the config changed.
    Restyle,
}

const STRIP_H: i32 = 58;
const EV_TOP: f64 = 6.0;
const EV_H: i32 = 28;

fn label(text: &str, classes: &[&str]) -> gtk::Label {
    let l = gtk::Label::new(Some(text));
    l.set_xalign(0.0);
    for c in classes {
        l.add_css_class(c);
    }
    l
}

fn button(text: &str, tip: &str) -> gtk::Button {
    let b = gtk::Button::with_label(text);
    b.set_tooltip_text(Some(tip));
    b.set_focus_on_click(false);
    b
}

fn send_quiet(req: Request) {
    std::thread::spawn(move || {
        let _ = timetable_core::protocol::request(&req);
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
    for c in ["free", "next", "soon", "now", "in", "focus", "offline"] {
        w.remove_css_class(c);
    }
    w.add_css_class(state);
}

fn local(t: i64, zone: Tz) -> DateTime<Tz> {
    DateTime::<Utc>::from_timestamp(t, 0)
        .unwrap_or_default()
        .with_timezone(&zone)
}

fn hhmm(t: i64, zone: Tz) -> String {
    local(t, zone).format("%H:%M").to_string()
}

/// `+8h`, `-5h30m`: a zone's offset from the local one.
fn offset(tz: Tz, zone: Tz, now: i64) -> String {
    use chrono::Offset;
    let at = |z: Tz| {
        local(now, zone)
            .with_timezone(&z)
            .offset()
            .fix()
            .local_minus_utc()
    };
    let diff = (at(tz) - at(zone)) / 60;
    if diff == 0 {
        return "same".into();
    }
    let sign = if diff > 0 { "+" } else { "-" };
    let (h, m) = (diff.abs() / 60, diff.abs() % 60);
    if m == 0 {
        format!("{sign}{h}h")
    } else {
        format!("{sign}{h}h{m:02}")
    }
}

/// What a button does.
type Action = Box<dyn Fn(&Rc<Drawer>)>;

struct ZoneTile {
    tz: Tz,
    meta: gtk::Label,
    big: gtk::Label,
}

pub struct Drawer {
    window: gtk::ApplicationWindow,
    stage: Stage,
    spring: Spring,
    visible: Cell<bool>,
    snap: RefCell<Option<Snapshot>>,
    cfg: RefCell<Config>,

    lamp: gtk::Label,
    sub: gtk::Label,
    chip: gtk::Label,
    allday: gtk::Label,
    notice: gtk::Label,

    zones: gtk::Box,
    tiles: RefCell<Vec<ZoneTile>>,
    zones_sig: RefCell<String>,

    next_card: gtk::Box,
    next_head: gtk::Label,
    next_kind: gtk::Label,
    next_title: gtk::Label,
    next_when: gtk::Label,
    next_then: gtk::Label,
    join_btn: gtk::Button,
    copy_btn: gtk::Button,
    next_event: RefCell<Option<Event>>,

    strip_aside: gtk::Label,
    strip: gtk::Fixed,
    rows: gtk::Box,
    day_sig: RefCell<String>,

    focus_card: gtk::Box,
    focus_aside: gtk::Label,
    focus_count: gtk::Label,
    focus_text: gtk::Label,
    focus_btn: gtk::Button,
    foot: gtk::Label,
}

impl Drawer {
    pub fn new(app: &gtk::Application) -> Rc<Drawer> {
        let (cfg, _) = Config::load();
        let window = gtk::ApplicationWindow::new(app);
        window.add_css_class("timetable");
        window.set_decorated(false);
        window.set_title(Some("Timetable"));
        window.init_layer_shell();
        window.set_layer(Layer::Overlay);
        window.set_namespace(Some("timetable"));
        for e in [LsEdge::Top, LsEdge::Bottom, LsEdge::Left, LsEdge::Right] {
            window.set_anchor(e, true);
        }
        window.set_exclusive_zone(0);
        window.set_keyboard_mode(KeyboardMode::Exclusive);

        // ---- header
        let lamp = label("\u{25cf}", &["lamp", "free"]);
        lamp.set_valign(gtk::Align::Center);
        let brand = label("TIMETABLE", &["brand"]);
        let sub = label("", &["sub"]);
        sub.set_ellipsize(gtk::pango::EllipsizeMode::End);
        let who = gtk::Box::new(gtk::Orientation::Vertical, 0);
        who.append(&brand);
        who.append(&sub);
        who.set_hexpand(true);
        let chip = label("free", &["chip", "free"]);
        chip.set_valign(gtk::Align::Center);
        let header = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        header.append(&lamp);
        header.append(&who);
        header.append(&chip);

        let allday = label("", &["allday"]);
        allday.set_wrap(true);
        allday.set_visible(false);
        let notice = label("", &["notice"]);
        notice.set_wrap(true);
        notice.set_visible(false);

        // ---- zones
        let zones = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        zones.set_homogeneous(true);

        // ---- next
        let next_head = label("NEXT", &["t-label"]);
        next_head.set_hexpand(true);
        let next_kind = label("", &["t-label"]);
        let nrow = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        nrow.append(&next_head);
        nrow.append(&next_kind);
        let next_title = label("", &["event-title"]);
        next_title.set_ellipsize(gtk::pango::EllipsizeMode::End);
        let next_when = label("", &["when"]);
        let next_then = label("", &["then"]);
        next_then.set_ellipsize(gtk::pango::EllipsizeMode::End);
        let join_btn = button(
            "Join",
            "Switch to the meeting workspace and open the link (j)",
        );
        let copy_btn = button("Copy link", "Copy the meeting link (c)");
        let btns = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        btns.add_css_class("btns");
        btns.append(&join_btn);
        btns.append(&copy_btn);
        let next_card = gtk::Box::new(gtk::Orientation::Vertical, 5);
        next_card.add_css_class("card");
        next_card.append(&nrow);
        next_card.append(&next_title);
        next_card.append(&next_when);
        next_card.append(&btns);
        next_card.append(&next_then);

        // ---- today
        let trow = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let tl = label("TODAY", &["t-label"]);
        tl.set_hexpand(true);
        let strip_aside = label("", &["t-label"]);
        trow.append(&tl);
        trow.append(&strip_aside);
        let strip = gtk::Fixed::new();
        strip.add_css_class("strip");
        strip.set_overflow(gtk::Overflow::Hidden);
        let rows = gtk::Box::new(gtk::Orientation::Vertical, 2);
        rows.add_css_class("rows");
        let today = gtk::Box::new(gtk::Orientation::Vertical, 6);
        today.append(&trow);
        today.append(&strip);
        today.append(&rows);

        // ---- focus
        let frow = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let fl = label("FOCUS", &["t-label", "head"]);
        fl.set_hexpand(true);
        let focus_aside = label("", &["t-label"]);
        frow.append(&fl);
        frow.append(&focus_aside);
        let focus_count = label("", &["countdown"]);
        let focus_text = label("", &[]);
        focus_text.set_wrap(true);
        let focus_btn = button("Start focus block", "Start or stop a focus block (f)");
        let fbtns = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        fbtns.add_css_class("btns");
        fbtns.append(&focus_btn);
        let focus_card = gtk::Box::new(gtk::Orientation::Vertical, 6);
        focus_card.add_css_class("card");
        focus_card.append(&frow);
        focus_card.append(&focus_count);
        focus_card.append(&focus_text);
        focus_card.append(&fbtns);

        let foot = label("", &["foot"]);
        foot.set_wrap(true);

        let panel = gtk::Box::new(gtk::Orientation::Vertical, 12);
        panel.add_css_class("panel");
        panel.append(&header);
        panel.append(&allday);
        panel.append(&notice);
        panel.append(&zones);
        panel.append(&next_card);
        panel.append(&today);
        panel.append(&focus_card);
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
            stage,
            spring,
            visible: Cell::new(false),
            snap: RefCell::new(None),
            cfg: RefCell::new(cfg),
            lamp,
            sub,
            chip,
            allday,
            notice,
            zones,
            tiles: RefCell::new(vec![]),
            zones_sig: RefCell::new(String::new()),
            next_card,
            next_head,
            next_kind,
            next_title,
            next_when,
            next_then,
            join_btn,
            copy_btn,
            next_event: RefCell::new(None),
            strip_aside,
            strip,
            rows,
            day_sig: RefCell::new(String::new()),
            focus_card,
            focus_aside,
            focus_count,
            focus_text,
            focus_btn,
            foot,
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
        let bind = |b: &gtk::Button, f: Action| {
            let w = Rc::downgrade(self);
            b.connect_clicked(move |_| {
                if let Some(d) = w.upgrade() {
                    f(&d);
                }
            });
        };
        bind(&self.join_btn, Box::new(|d| d.join()));
        bind(&self.copy_btn, Box::new(|d| d.copy()));
        bind(
            &self.focus_btn,
            Box::new(|_| {
                send_quiet(Request::Focus {
                    action: FocusAction::Toggle,
                })
            }),
        );
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
        // The clocks and countdowns move while the drawer is open.
        let w = Rc::downgrade(self);
        glib::timeout_add_local(Duration::from_millis(1000), move || {
            let Some(d) = w.upgrade() else {
                return glib::ControlFlow::Break;
            };
            if d.visible.get() {
                d.render();
            }
            glib::ControlFlow::Continue
        });
    }

    pub fn handle(self: &Rc<Self>, msg: Msg) {
        match msg {
            Msg::Event(BoardEvent::Snapshot { snapshot }) => {
                *self.snap.borrow_mut() = Some(*snapshot);
                self.day_sig.borrow_mut().clear();
                if self.visible.get() {
                    self.render();
                }
            }
            Msg::Event(BoardEvent::Show { anchor }) => self.show(anchor),
            Msg::Event(BoardEvent::Toggle { anchor }) => {
                if self.visible.get() {
                    self.hide();
                } else {
                    self.show(anchor);
                }
            }
            Msg::Event(BoardEvent::Hide) => self.hide(),
            Msg::Restyle => {
                crate::linux::apply_styles();
                let (cfg, _) = Config::load();
                self.stage
                    .set_geometry(cfg.drawer.width, cfg.drawer.max_height, cfg.drawer.gap);
                *self.cfg.borrow_mut() = cfg;
                self.zones_sig.borrow_mut().clear();
                self.day_sig.borrow_mut().clear();
                if self.visible.get() {
                    self.render();
                }
            }
        }
    }

    fn key(self: &Rc<Self>, key: gdk::Key) -> glib::Propagation {
        match key {
            gdk::Key::Escape => self.hide(),
            gdk::Key::j => self.join(),
            gdk::Key::c => self.copy(),
            gdk::Key::f => send_quiet(Request::Focus {
                action: FocusAction::Toggle,
            }),
            gdk::Key::r => send_quiet(Request::Refresh),
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
        self.stage.set_anchor(anchor);
        let was_visible = self.visible.replace(true);
        self.render();
        self.window.set_visible(true);
        self.window.present();
        if !was_visible {
            self.spring.play_in(0);
            send_quiet(Request::BoardVisible { visible: true });
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

    /// Joins the meeting on the card. The daemon closes the drawer first
    /// (an open drawer keeps Hyprland on its workspace), then switches
    /// workspace and opens the link.
    fn join(self: &Rc<Self>) {
        let Some(e) = self.next_event.borrow().clone() else {
            return;
        };
        if e.link.is_none() {
            return;
        }
        send_quiet(Request::Join {
            key: Some(e.key()),
            dry_run: false,
        });
    }

    fn copy(self: &Rc<Self>) {
        let Some(link) = self
            .next_event
            .borrow()
            .as_ref()
            .and_then(|e| e.link.clone())
        else {
            return;
        };
        if let Some(display) = gdk::Display::default() {
            display.clipboard().set_text(&link);
            self.copy_btn.set_label("Copied");
            let b = self.copy_btn.clone();
            glib::timeout_add_local_once(Duration::from_millis(1500), move || {
                b.set_label("Copy link");
            });
        }
    }

    fn render(&self) {
        let snap = self.snap.borrow();
        let cfg = self.cfg.borrow();
        let zone = cfg.local_zone();
        let now = timetable_core::now_secs();
        self.render_zones(&cfg, zone, now);
        let Some(s) = snap.as_ref() else {
            self.sub.set_label("waiting for the daemon");
            self.next_card.set_visible(false);
            self.focus_card.set_visible(false);
            return;
        };
        let st = agenda::status(
            &s.events,
            now,
            s.focus.as_ref(),
            &s.joined,
            s.has_calendar(),
            s.source != Source::None,
            &cfg,
            zone,
        );
        self.render_header(s, &st, zone, now);
        self.render_next(s, &st, zone, now);
        self.render_day(s, &cfg, zone, now);
        self.render_focus(s, &cfg, zone, now);
        self.foot.set_label(&format!(
            "Esc close \u{b7} j join \u{b7} f focus \u{b7} r refresh \u{b7} {}",
            cfg.drawer.calendar_hint
        ));
    }

    fn render_header(&self, s: &Snapshot, st: &agenda::Status, zone: Tz, now: i64) {
        let today = agenda::on_day(&s.events, local_date(now, zone), zone);
        let timed = today.iter().filter(|e| !e.all_day).count();
        let fresh = match (s.source, s.fetched_at) {
            (Source::None, _) => "no calendar configured".to_string(),
            (_, None) if s.error.is_some() => "calendar unreachable".into(),
            (_, None) => "reading the calendar".into(),
            (_, Some(f)) => format!("refreshed {}", ago(now - f as i64)),
        };
        self.sub.set_label(&format!(
            "{} \u{b7} {} \u{b7} {fresh}",
            local(now, zone).format("%a %b %-d"),
            match timed {
                1 => "1 event".to_string(),
                n => format!("{n} events"),
            }
        ));
        let state = st.state.as_str();
        set_state_class(&self.lamp, state);
        set_state_class(&self.chip, state);
        let until = |e: &Option<Event>| {
            e.as_ref()
                .map(|e| minutes(agenda::minutes_until(e.start, now)))
                .unwrap_or_default()
        };
        self.chip.set_label(&match st.state {
            State::Free => "free".to_string(),
            State::Next | State::Soon => format!("next {}", until(&st.event)),
            State::Now => "starting".into(),
            State::In => "in a meeting".into(),
            State::Focus => "focus".into(),
            State::Offline => "offline".into(),
        });
        let all: Vec<&str> = today
            .iter()
            .filter(|e| e.all_day)
            .map(|e| e.title_or_default())
            .collect();
        self.allday.set_visible(!all.is_empty());
        self.allday
            .set_label(&format!("All day: {}", all.join(" \u{b7} ")));
        let mut notes = vec![];
        if let Some(e) = &s.config_error {
            notes.push(format!("Config problem, using defaults: {e}"));
        }
        if let Some(e) = &s.error {
            notes.push(match s.fetched_at {
                Some(_) => format!("Calendar refresh failing, showing the last good copy: {e}"),
                None => format!("Calendar unreachable: {e}"),
            });
        }
        self.notice.set_visible(!notes.is_empty());
        self.notice.set_label(&notes.join("\n"));
    }

    fn render_zones(&self, cfg: &Config, zone: Tz, now: i64) {
        let zones = cfg.zones();
        let sig = format!("{zones:?}");
        if *self.zones_sig.borrow() != sig {
            *self.zones_sig.borrow_mut() = sig;
            while let Some(c) = self.zones.first_child() {
                self.zones.remove(&c);
            }
            let mut tiles = vec![];
            for (tz, name) in zones {
                let card = gtk::Box::new(gtk::Orientation::Vertical, 2);
                card.add_css_class("card");
                card.add_css_class("zone");
                if tz == zone {
                    card.add_css_class("local");
                }
                let row = gtk::Box::new(gtk::Orientation::Horizontal, 4);
                let n = label(&name.to_uppercase(), &["t-label"]);
                n.set_hexpand(true);
                n.set_ellipsize(gtk::pango::EllipsizeMode::End);
                let meta = label("", &["t-label"]);
                row.append(&n);
                row.append(&meta);
                let big = label("", &["big"]);
                card.append(&row);
                card.append(&big);
                self.zones.append(&card);
                tiles.push(ZoneTile { tz, meta, big });
            }
            *self.tiles.borrow_mut() = tiles;
        }
        for t in self.tiles.borrow().iter() {
            let at = local(now, zone).with_timezone(&t.tz);
            t.big.set_label(&at.format("%H:%M").to_string());
            t.meta.set_label(&if t.tz == zone {
                at.format("%Z").to_string()
            } else {
                offset(t.tz, zone, now)
            });
            let day = at
                .date_naive()
                .signed_duration_since(local(now, zone).date_naive())
                .num_days();
            t.big.set_tooltip_text(Some(&format!(
                "{}{}",
                at.format("%a %b %-d %H:%M %Z"),
                match day {
                    1 => " (tomorrow)",
                    -1 => " (yesterday)",
                    _ => "",
                }
            )));
        }
    }

    fn render_next(&self, s: &Snapshot, st: &agenda::Status, zone: Tz, now: i64) {
        let card = &self.next_card;
        if s.source == Source::None {
            card.set_visible(false);
            *self.next_event.borrow_mut() = None;
            return;
        }
        card.set_visible(true);
        let current = agenda::current(&s.events, now)
            .first()
            .map(|e| (*e).clone());
        let upcoming = agenda::upcoming(&s.events, now).cloned();
        let (head, ev) = match st.state {
            State::Now | State::In => ("NOW", current.clone()),
            _ => ("NEXT", upcoming.clone()),
        };
        for c in ["soon", "now"] {
            card.remove_css_class(c);
            self.next_head.remove_css_class(c);
        }
        match st.state {
            State::Soon => {
                card.add_css_class("soon");
                self.next_head.add_css_class("soon");
            }
            State::Now => {
                card.add_css_class("now");
                self.next_head.add_css_class("now");
            }
            _ => {}
        }
        self.next_head.set_label(head);
        let Some(e) = ev else {
            self.next_kind.set_label("");
            self.next_title.set_label("Nothing else on the calendar");
            self.next_when.set_visible(false);
            self.join_btn.set_visible(false);
            self.copy_btn.set_visible(false);
            self.next_then.set_visible(false);
            *self.next_event.borrow_mut() = None;
            return;
        };
        self.next_kind.set_label(
            &e.kind
                .as_deref()
                .map(|k| links::label(k).to_uppercase())
                .unwrap_or_else(|| "NO LINK".into()),
        );
        self.next_title.set_label(e.title_or_default());
        self.next_title.set_tooltip_text(Some(e.title_or_default()));
        let same_day = local_date(e.start, zone) == local_date(now, zone);
        let range = format!("{} to {}", hhmm(e.start, zone), hhmm(e.end, zone));
        let when = if e.running(now) {
            format!(
                "{range} \u{b7} {} left",
                minutes(agenda::minutes_until(e.end, now))
            )
        } else if same_day {
            format!(
                "{range} \u{b7} in {}",
                minutes(agenda::minutes_until(e.start, now))
            )
        } else {
            format!(
                "{} \u{b7} {range}",
                local(e.start, zone).format("%a %b %-d")
            )
        };
        let when = if e.location.is_empty() || e.location.starts_with("http") {
            when
        } else {
            format!("{when} \u{b7} {}", e.location)
        };
        self.next_when.set_label(&when);
        self.next_when.set_visible(true);
        let has_link = e.link.is_some();
        self.join_btn.set_visible(has_link);
        self.copy_btn.set_visible(has_link);
        let urgent = matches!(st.state, State::Soon | State::Now);
        if urgent {
            self.join_btn.add_css_class("primary");
        } else {
            self.join_btn.remove_css_class("primary");
        }
        self.join_btn
            .set_label(&match cfg_workspace(&self.cfg.borrow()) {
                Some(ws) => format!("Join on workspace {ws}"),
                None => "Join".into(),
            });
        let then = if head == "NOW" {
            upcoming
                .filter(|u| u.key() != e.key())
                .map(|u| format!("Then {} at {}", u.title_or_default(), hhmm(u.start, zone)))
        } else {
            None
        };
        self.next_then.set_visible(then.is_some());
        self.next_then.set_label(&then.unwrap_or_default());
        *self.next_event.borrow_mut() = Some(e);
    }

    fn render_day(&self, s: &Snapshot, cfg: &Config, zone: Tz, now: i64) {
        let day = local_date(now, zone);
        let today = agenda::on_day(&s.events, day, zone);
        let timed: Vec<&Event> = today.into_iter().filter(|e| !e.all_day).collect();
        // The configured span, widened to fit today's events.
        let (mut s0, mut s1) = agenda::day_span(day, cfg, zone);
        for e in &timed {
            s0 = s0.min(e.start - e.start.rem_euclid(3600));
            s1 = s1.max(e.end + (3600 - e.end.rem_euclid(3600)) % 3600);
        }
        let midnight = timetable_core::expand::midnight(day, zone);
        s0 = s0.max(midnight);
        s1 = s1.min(midnight + 86400).max(s0 + 3600);
        let minute = now / 60;
        let sig = format!("{minute}|{s0}|{s1}|{:?}|{}", s.focus, timed.len());
        if *self.day_sig.borrow() == sig {
            return;
        }
        *self.day_sig.borrow_mut() = sig;
        let free = agenda::gaps(&s.events, (s0, s1), cfg.day.min_gap)
            .iter()
            .map(|(a, b)| (b - a) / 60)
            .sum::<i64>();
        self.strip_aside.set_label(&format!(
            "{} TO {} \u{b7} {} FREE",
            hhmm(s0, zone),
            hhmm(s1, zone),
            minutes(free).to_uppercase()
        ));

        // ---- the strip
        while let Some(c) = self.strip.first_child() {
            self.strip.remove(&c);
        }
        let w = (cfg.drawer.width - 30.0).max(200.0);
        self.strip.set_size_request(w as i32, STRIP_H);
        let x = |t: i64| ((t - s0) as f64 / (s1 - s0) as f64 * w).clamp(0.0, w);
        let next = agenda::upcoming(&s.events, now).map(|e| e.key());
        for (a, b) in agenda::gaps(&s.events, (s0, s1), cfg.day.min_gap) {
            let g = gtk::Box::new(gtk::Orientation::Horizontal, 0);
            g.add_css_class("gap");
            if s.focus
                .is_some_and(|f| f.running(now) && f.start < b && f.end > a)
            {
                g.add_css_class("focus");
            }
            let gw = (x(b) - x(a) - 2.0).max(2.0);
            g.set_size_request(gw as i32, EV_H);
            g.set_tooltip_text(Some(&format!(
                "Free {} to {} ({})",
                hhmm(a, zone),
                hhmm(b, zone),
                minutes((b - a) / 60)
            )));
            self.strip.put(&g, x(a) + 1.0, EV_TOP);
        }
        for e in &timed {
            let (a, b) = (e.start.max(s0), e.end.min(s1));
            if b <= a {
                continue;
            }
            let ew = (x(b) - x(a) - 1.0).max(3.0);
            let chars = ((ew - 8.0) / 6.0).floor() as i32;
            let l = label(if chars >= 2 { e.title_or_default() } else { "" }, &["ev"]);
            l.set_ellipsize(gtk::pango::EllipsizeMode::End);
            l.set_max_width_chars(chars.max(0));
            l.set_width_chars(0);
            l.set_size_request(ew as i32, EV_H);
            if e.running(now) {
                l.add_css_class("current");
            } else if e.end <= now {
                l.add_css_class("past");
            } else if next.as_deref() == Some(&e.key()) {
                l.add_css_class("next");
            }
            l.set_tooltip_text(Some(&format!(
                "{} to {}  {}",
                hhmm(e.start, zone),
                hhmm(e.end, zone),
                e.title_or_default()
            )));
            self.strip.put(&l, x(a), EV_TOP);
        }
        if now > s0 && now < s1 {
            let n = gtk::Box::new(gtk::Orientation::Horizontal, 0);
            n.add_css_class("nowline");
            n.set_size_request(2, EV_H + 8);
            self.strip.put(&n, x(now) - 1.0, EV_TOP - 4.0);
        }
        let first_hour = s0 + (3600 - s0.rem_euclid(3600)) % 3600;
        let span_h = (s1 - s0) / 3600;
        let step = if span_h > 12 { 3 } else { 2 } * 3600;
        let mut t = first_hour;
        while t <= s1 {
            let h = local(t, zone).format("%-H").to_string();
            if local(t, zone)
                .format("%H")
                .to_string()
                .parse::<i64>()
                .unwrap_or(0)
                % (step / 3600)
                == 0
            {
                let l = label(&h, &["tick"]);
                let lx = (x(t) - 3.0).clamp(1.0, w - 12.0);
                self.strip.put(&l, lx, (EV_TOP as i32 + EV_H + 6) as f64);
            }
            t += 3600;
        }

        // ---- the rows
        while let Some(c) = self.rows.first_child() {
            self.rows.remove(&c);
        }
        // What is left today, plus the one just finished.
        let first = timed
            .iter()
            .position(|e| e.end > now)
            .unwrap_or(timed.len());
        let from = first.saturating_sub(1);
        for e in timed.iter().skip(from).take(7) {
            let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
            let t = label(
                &format!("{} to {}", hhmm(e.start, zone), hhmm(e.end, zone)),
                &["time"],
            );
            let title = label(e.title_or_default(), &[]);
            title.set_hexpand(true);
            title.set_ellipsize(gtk::pango::EllipsizeMode::End);
            let kind = label(e.kind.as_deref().map(links::label).unwrap_or(""), &["kind"]);
            if e.end <= now {
                t.add_css_class("past");
                title.add_css_class("past");
            } else if e.running(now) {
                title.add_css_class("current");
            }
            row.append(&t);
            row.append(&title);
            row.append(&kind);
            self.rows.append(&row);
        }
        if timed.is_empty() {
            self.rows
                .append(&label("Nothing on the calendar today.", &["dim"]));
        } else if first == timed.len() {
            self.rows
                .append(&label("That was the last one today.", &["dim"]));
        }
    }

    fn render_focus(&self, s: &Snapshot, cfg: &Config, zone: Tz, now: i64) {
        let card = &self.focus_card;
        card.set_visible(true);
        match s.focus.filter(|f| f.running(now)) {
            Some(f) => {
                card.add_css_class("focus-on");
                self.focus_aside
                    .set_label(&format!("UNTIL {}", hhmm(f.end, zone)));
                self.focus_count.set_visible(true);
                self.focus_count.set_label(&countdown(f.end - now));
                let after = agenda::upcoming(&s.events, f.end - 1)
                    .filter(|e| e.start - f.end <= cfg.focus.lead * 60 + 60);
                self.focus_text.set_label(&match after {
                    Some(e) => format!(
                        "Started at {}. Ends {} min before {}.",
                        hhmm(f.start, zone),
                        (e.start - f.end) / 60,
                        e.title_or_default()
                    ),
                    None => format!("Started at {}.", hhmm(f.start, zone)),
                });
                self.focus_btn.set_label("Stop");
                self.focus_btn.add_css_class("danger");
                self.focus_btn.set_sensitive(true);
            }
            None => {
                card.remove_css_class("focus-on");
                self.focus_count.set_visible(false);
                self.focus_btn.remove_css_class("danger");
                self.focus_btn.set_label("Start focus block");
                match agenda::focus_plan(&s.events, now, cfg, zone) {
                    Ok(p) => {
                        self.focus_aside.set_label(&format!("{} MIN", p.minutes()));
                        let mut text = match &p.before {
                            Some(e) => format!(
                                "Free until {}: {} min of focus, warned at {}.",
                                hhmm(e.start, zone),
                                p.minutes(),
                                hhmm(p.end, zone)
                            ),
                            None => format!(
                                "Nothing else today: {} min of focus, until {}.",
                                p.minutes(),
                                hhmm(p.end, zone)
                            ),
                        };
                        if p.minutes() < 25 {
                            text.push_str(&self.long_gap(s, cfg, zone, now));
                        }
                        self.focus_text.set_label(&text);
                        self.focus_btn.set_sensitive(true);
                    }
                    Err(e) => {
                        self.focus_aside.set_label("");
                        let mut text = format!("{e}.");
                        text.push_str(&self.long_gap(s, cfg, zone, now));
                        self.focus_text.set_label(&text);
                        self.focus_btn.set_sensitive(false);
                    }
                }
            }
        }
    }

    /// " Next long gap: 15:00 to 16:30." for when the gap now is short.
    fn long_gap(&self, s: &Snapshot, cfg: &Config, zone: Tz, now: i64) -> String {
        let day = local_date(now, zone);
        let span = agenda::day_span(day, cfg, zone);
        agenda::gaps(&s.events, (now.max(span.0), span.1.max(now)), 30)
            .into_iter()
            .find(|(a, _)| *a > now + 60)
            .map(|(a, b)| {
                format!(
                    " Next long gap: {} to {} ({}).",
                    hhmm(a, zone),
                    hhmm(b, zone),
                    minutes((b - a) / 60)
                )
            })
            .unwrap_or_default()
    }
}

fn cfg_workspace(cfg: &Config) -> Option<String> {
    let ws = cfg.join.workspace.trim();
    (!ws.is_empty()).then(|| ws.to_string())
}
