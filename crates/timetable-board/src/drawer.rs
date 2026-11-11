//! The drawer: the date and the state, three zone tiles, the next (or
//! current) meeting with Join and Copy link, the day with its free gaps and
//! a now line, and the focus card. The day is a strip across the drawer
//! with the rest of today listed under it, or a timeline beside the rest;
//! a button and `v` switch between them.

use crate::anim::{Motion, Spring};
use crate::canvas::{Axis, Canvas, Place};
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
use timetable_core::model::{Answer, Event, Focus, Snapshot, Source};
use timetable_core::paths;
use timetable_core::protocol::{Event as BoardEvent, FocusAction, Request};
use timetable_core::text::{ago, countdown, minutes};

pub enum Msg {
    Event(BoardEvent),
    /// A watched stylesheet or the config changed.
    Restyle,
}

// The strip, top to bottom: the now tag, the event lanes, the hour ticks.
const TAG_H: f64 = 15.0;
const LANE_H: f64 = 24.0;
const TICK_H: f64 = 14.0;
// The timeline: its hour column, and the inset above and below the day.
const HOURS_W: i32 = 40;
const TL_PAD: f64 = 8.0;

/// How the drawer shows the day.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum View {
    /// A strip across the drawer, the rest of today listed under it.
    Strip,
    /// A timeline in its own column, beside the zones, Now and Focus.
    Timeline,
}

impl View {
    fn load() -> View {
        match std::fs::read_to_string(paths::view_file()) {
            Ok(s) if s.trim() == "timeline" => View::Timeline,
            _ => View::Strip,
        }
    }

    fn save(self) {
        let _ = std::fs::create_dir_all(paths::state_dir());
        let _ = std::fs::write(
            paths::view_file(),
            match self {
                View::Strip => "strip\n",
                View::Timeline => "timeline\n",
            },
        );
    }

    fn other(self) -> View {
        match self {
            View::Strip => View::Timeline,
            View::Timeline => View::Strip,
        }
    }
}

/// Today as the strip and the timeline draw it.
struct Day<'a> {
    s0: i64,
    s1: i64,
    now: i64,
    zone: Tz,
    timed: &'a [&'a Event],
    /// Each timed event's lane and its overlapping group's lane count.
    lanes: Vec<(usize, usize)>,
    /// Where nothing is on from now on, drawn dashed.
    gaps: Vec<(i64, i64)>,
    next: Option<String>,
    focus: Option<Focus>,
}

impl Day<'_> {
    /// Where `t` falls in the day, 0 to 1.
    fn f(&self, t: i64) -> f64 {
        ((t - self.s0) as f64 / (self.s1 - self.s0) as f64).clamp(0.0, 1.0)
    }

    /// Whole hours in the day.
    fn hours(&self) -> impl Iterator<Item = i64> + '_ {
        let first = self.s0 + (3600 - self.s0.rem_euclid(3600)) % 3600;
        (0..)
            .map(move |i| first + i * 3600)
            .take_while(move |t| *t <= self.s1)
    }

    fn event(&self, e: &Event, text: &str) -> gtk::Label {
        let l = label(text, &["ev"]);
        l.set_ellipsize(gtk::pango::EllipsizeMode::End);
        if e.running(self.now) {
            l.add_css_class("current");
        } else if e.end <= self.now {
            l.add_css_class("past");
        } else if self.next.as_deref() == Some(&e.key()) {
            l.add_css_class("next");
        }
        if e.own {
            l.add_css_class("own");
        }
        match e.answer {
            Answer::Yes => {}
            Answer::Maybe => l.add_css_class("maybe"),
            Answer::Unanswered => l.add_css_class("unanswered"),
        }
        let note = match (e.own, e.answer.label()) {
            (true, _) => " (own time)".to_string(),
            (false, Some(a)) => format!(" ({a})"),
            (false, None) => String::new(),
        };
        l.set_tooltip_text(Some(&format!(
            "{} to {}  {}{note}",
            hhmm(e.start, self.zone),
            hhmm(e.end, self.zone),
            e.title_or_default()
        )));
        l
    }

    /// A dashed free gap saying how long it is.
    fn gap(&self, a: i64, b: i64) -> gtk::Box {
        let g = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        g.add_css_class("gap");
        if self.focus.is_some_and(|f| f.start < b && f.end > a) {
            g.add_css_class("focus");
        }
        let l = label(&format!("{} free", minutes((b - a) / 60)), &[]);
        l.set_ellipsize(gtk::pango::EllipsizeMode::End);
        l.set_hexpand(true);
        g.append(&l);
        g.set_tooltip_text(Some(&format!(
            "Free {} to {} ({})",
            hhmm(a, self.zone),
            hhmm(b, self.zone),
            minutes((b - a) / 60)
        )));
        g
    }
}

fn rule(w: i32, h: i32, class: &str) -> gtk::Box {
    let b = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    b.add_css_class(class);
    b.set_size_request(w, h);
    b
}

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
    for c in [
        "free", "next", "soon", "now", "in", "own", "focus", "offline",
    ] {
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

    view: Cell<View>,
    /// Days from today the day view shows; back to 0 when the drawer
    /// closes.
    offset: Cell<i64>,
    day_name: gtk::Label,
    prev_btn: gtk::Button,
    next_btn: gtk::Button,
    today_btn: gtk::Button,
    body: gtk::Box,
    cols: gtk::Box,
    left: gtk::Box,
    right: gtk::Box,
    today: gtk::Box,
    view_btn: gtk::Button,
    strip_aside: gtk::Label,
    strip: Canvas,
    timeline: gtk::Box,
    hours: Canvas,
    lanes: Canvas,
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
        next_when.set_ellipsize(gtk::pango::EllipsizeMode::End);
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
        let prev_btn = button("\u{2039}", "The day before (\u{2190})");
        let next_btn = button("\u{203a}", "The day after (\u{2192})");
        let today_btn = button("Today", "Back to today (t)");
        let nav = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        nav.add_css_class("btns");
        nav.add_css_class("view");
        nav.append(&prev_btn);
        nav.append(&next_btn);
        let tl = label("TODAY", &["t-label", "day-name"]);
        tl.set_hexpand(true);
        let back = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        back.add_css_class("btns");
        back.add_css_class("view");
        back.append(&today_btn);
        let strip_aside = label("", &["t-label"]);
        strip_aside.set_valign(gtk::Align::Center);
        let view_btn = button("", "");
        let vbox = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        vbox.add_css_class("btns");
        vbox.add_css_class("view");
        vbox.append(&view_btn);
        tl.set_valign(gtk::Align::Center);
        trow.append(&nav);
        trow.append(&tl);
        trow.append(&back);
        trow.append(&strip_aside);
        trow.append(&vbox);
        let strip = Canvas::new();
        strip.add_css_class("strip");
        let hours = Canvas::new();
        hours.set_size_request(HOURS_W, -1);
        let lanes = Canvas::new();
        lanes.set_hexpand(true);
        let timeline = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        timeline.add_css_class("strip");
        timeline.add_css_class("timeline");
        timeline.append(&hours);
        timeline.append(&lanes);
        let rows = gtk::Box::new(gtk::Orientation::Vertical, 2);
        rows.add_css_class("rows");
        let today = gtk::Box::new(gtk::Orientation::Vertical, 6);
        today.append(&trow);
        today.append(&strip);
        today.append(&timeline);
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

        // The strip view stacks everything in `body`; the timeline view
        // puts the day in the right column of `cols`. `apply_view` moves
        // the parts between them.
        let body = gtk::Box::new(gtk::Orientation::Vertical, 12);
        let left = gtk::Box::new(gtk::Orientation::Vertical, 12);
        let right = gtk::Box::new(gtk::Orientation::Vertical, 12);
        let cols = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        cols.set_homogeneous(true);
        cols.append(&left);
        cols.append(&right);

        let panel = gtk::Box::new(gtk::Orientation::Vertical, 12);
        panel.add_css_class("panel");
        panel.append(&header);
        panel.append(&allday);
        panel.append(&notice);
        panel.append(&body);
        panel.append(&cols);
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
            view: Cell::new(View::load()),
            offset: Cell::new(0),
            day_name: tl,
            prev_btn,
            next_btn,
            today_btn,
            body,
            cols,
            left,
            right,
            today,
            view_btn,
            strip_aside,
            strip,
            timeline,
            hours,
            lanes,
            rows,
            day_sig: RefCell::new(String::new()),
            focus_card,
            focus_aside,
            focus_count,
            focus_text,
            focus_btn,
            foot,
        });
        d.apply_view();
        d.connect();
        d
    }

    /// Puts the parts where the current view wants them.
    fn apply_view(&self) {
        let view = self.view.get();
        let parts: [gtk::Widget; 4] = [
            self.zones.clone().upcast(),
            self.next_card.clone().upcast(),
            self.today.clone().upcast(),
            self.focus_card.clone().upcast(),
        ];
        for w in &parts {
            if let Some(p) = w.parent().and_downcast::<gtk::Box>() {
                p.remove(w);
            }
        }
        let [zones, next, today, focus] = &parts;
        match view {
            View::Strip => {
                for w in &parts {
                    self.body.append(w);
                }
            }
            View::Timeline => {
                self.left.append(zones);
                self.left.append(next);
                self.left.append(focus);
                self.right.append(today);
            }
        }
        let strip = view == View::Strip;
        self.body.set_visible(strip);
        self.cols.set_visible(!strip);
        self.strip.set_visible(strip);
        self.rows.set_visible(strip);
        self.timeline.set_visible(!strip);
        let (text, tip) = match view {
            View::Strip => ("Timeline", "Show the day as a timeline beside the rest (v)"),
            View::Timeline => ("Strip", "Show the day as a strip across the drawer (v)"),
        };
        self.view_btn.set_label(text);
        self.view_btn.set_tooltip_text(Some(tip));
        self.day_sig.borrow_mut().clear();
    }

    /// Moves the day view by `by` days, within the agenda's window.
    fn step_day(&self, by: i64) {
        let cal = &self.cfg.borrow().calendar;
        let (lo, hi) = (-(cal.past_days as i64), cal.future_days as i64 - 1);
        let to = (self.offset.get() + by).clamp(lo, hi);
        if to == self.offset.get() {
            return;
        }
        self.offset.set(to);
        self.day_sig.borrow_mut().clear();
        if self.visible.get() {
            self.render();
        }
    }

    fn toggle_view(&self) {
        let view = self.view.get().other();
        self.view.set(view);
        view.save();
        self.apply_view();
        if self.visible.get() {
            self.render();
        }
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
        bind(&self.view_btn, Box::new(|d| d.toggle_view()));
        bind(&self.prev_btn, Box::new(|d| d.step_day(-1)));
        bind(&self.next_btn, Box::new(|d| d.step_day(1)));
        bind(&self.today_btn, Box::new(|d| d.step_day(-d.offset.get())));
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
            gdk::Key::v => self.toggle_view(),
            gdk::Key::Left => self.step_day(-1),
            gdk::Key::Right => self.step_day(1),
            gdk::Key::t => self.step_day(-self.offset.get()),
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
        // A look at another day is brief: the next open is today again.
        self.offset.set(0);
        self.day_sig.borrow_mut().clear();
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
            "Esc close \u{b7} j join \u{b7} f focus \u{b7} v view \u{b7} \u{2190} \u{2192} days \u{b7} r refresh \u{b7} {}",
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
            State::Own => "own time".into(),
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
        let upcoming = agenda::upcoming(&s.events, now).cloned();
        let (head, ev) = match st.state {
            State::Now | State::In | State::Own => ("NOW", st.event.clone()),
            _ => ("NEXT", upcoming.clone()),
        };
        for c in ["soon", "now", "own"] {
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
            State::Own => {
                card.add_css_class("own");
                self.next_head.add_css_class("own");
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
        let kind = match &e.kind {
            Some(k) => links::label(k).to_uppercase(),
            None if e.own => "OWN TIME".into(),
            None => "NO LINK".into(),
        };
        self.next_kind.set_label(&match e.answer.label() {
            Some(a) => format!("{} \u{b7} {kind}", a.to_uppercase()),
            None => kind,
        });
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
        self.next_when.set_tooltip_text(Some(&when));
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
        let offset = self.offset.get();
        let today = local_date(now, zone);
        let day = today
            .checked_add_signed(chrono::Duration::days(offset))
            .unwrap_or(today);
        let on_day = agenda::on_day(&s.events, day, zone);
        let all_day: Vec<&str> = on_day
            .iter()
            .filter(|e| e.all_day)
            .map(|e| e.title_or_default())
            .collect();
        let timed: Vec<&Event> = on_day.into_iter().filter(|e| !e.all_day).collect();
        // The configured span, widened to fit today's events.
        let (mut s0, mut s1) = agenda::day_span(day, cfg, zone);
        for e in &timed {
            s0 = s0.min(e.start - e.start.rem_euclid(3600));
            s1 = s1.max(e.end + (3600 - e.end.rem_euclid(3600)) % 3600);
        }
        let midnight = timetable_core::expand::midnight(day, zone);
        s0 = s0.max(midnight);
        s1 = s1.min(midnight + 86400).max(s0 + 3600);
        let view = self.view.get();
        let minute = now / 60;
        let sig = format!(
            "{view:?}|{offset}|{minute}|{s0}|{s1}|{:?}|{}",
            s.focus,
            timed.len()
        );
        if *self.day_sig.borrow() == sig {
            return;
        }
        *self.day_sig.borrow_mut() = sig;
        let cal = &cfg.calendar;
        self.prev_btn
            .set_sensitive(offset > -(cal.past_days as i64));
        self.next_btn
            .set_sensitive(offset < cal.future_days as i64 - 1);
        self.today_btn
            .parent()
            .inspect(|p| p.set_visible(offset != 0));
        self.day_name.set_label(&match offset {
            0 => "TODAY".to_string(),
            1 => "TOMORROW".into(),
            -1 => "YESTERDAY".into(),
            _ => day.format("%a %b %-d").to_string().to_uppercase(),
        });
        // Free time from now on: your own time counts, and a day gone by
        // has none left.
        let ahead = (now.max(s0), s1.max(now));
        let free = agenda::gaps(&s.events, ahead, cfg.day.min_gap)
            .iter()
            .map(|(a, b)| (b - a) / 60)
            .sum::<i64>();
        let span = format!("{} TO {}", hhmm(s0, zone), hhmm(s1, zone));
        self.strip_aside.set_label(&match offset.signum() {
            0 => format!("{span} \u{b7} {} FREE LEFT", minutes(free).to_uppercase()),
            1 => format!("{span} \u{b7} {} FREE", minutes(free).to_uppercase()),
            _ => span,
        });
        let d = Day {
            s0,
            s1,
            now,
            zone,
            timed: &timed,
            lanes: agenda::lanes(&timed),
            gaps: agenda::empty_gaps(&s.events, ahead, cfg.day.min_gap),
            next: agenda::upcoming(&s.events, now).map(|e| e.key()),
            focus: s.focus.filter(|f| f.running(now)),
        };
        match view {
            View::Strip => self.render_strip(&d),
            View::Timeline => {
                self.render_timeline(&d);
                return;
            }
        }

        // ---- the rows
        while let Some(c) = self.rows.first_child() {
            self.rows.remove(&c);
        }
        // Today: what is left, plus the one just finished. Another day:
        // all of it, its all-day events first.
        if offset != 0 && !all_day.is_empty() {
            self.rows.append(&label(
                &format!("All day: {}", all_day.join(" \u{b7} ")),
                &["dim"],
            ));
        }
        let first = match offset {
            0 => timed
                .iter()
                .position(|e| e.end > now)
                .unwrap_or(timed.len()),
            _ => 0,
        };
        let from = first.saturating_sub(1);
        for e in timed
            .iter()
            .skip(from)
            .take(if offset == 0 { 7 } else { 12 })
        {
            let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
            let t = label(
                &format!("{} to {}", hhmm(e.start, zone), hhmm(e.end, zone)),
                &["time"],
            );
            let title = label(e.title_or_default(), &[]);
            title.set_hexpand(true);
            title.set_ellipsize(gtk::pango::EllipsizeMode::End);
            let kind = e.kind.as_deref().map(links::label).unwrap_or("");
            let note = match (e.own, e.answer.label()) {
                (true, _) => "own time".to_string(),
                (false, Some(a)) if kind.is_empty() => a.to_string(),
                (false, Some(a)) => format!("{a} \u{b7} {kind}"),
                (false, None) => kind.to_string(),
            };
            let kind = label(&note, &["kind"]);
            if e.end <= now {
                t.add_css_class("past");
                title.add_css_class("past");
            } else if e.running(now) {
                title.add_css_class("current");
            }
            for c in [
                e.own.then_some("own"),
                (e.answer == Answer::Maybe).then_some("maybe"),
                (e.answer == Answer::Unanswered).then_some("unanswered"),
            ]
            .into_iter()
            .flatten()
            {
                title.add_css_class(c);
            }
            row.append(&t);
            row.append(&title);
            row.append(&kind);
            self.rows.append(&row);
        }
        if timed.is_empty() {
            let when = match offset {
                0 => "today".to_string(),
                1 => "tomorrow".into(),
                -1 => "yesterday".into(),
                _ => day.format("on %A").to_string(),
            };
            self.rows.append(&label(
                &format!("Nothing on the calendar {when}."),
                &["dim"],
            ));
        } else if offset == 0 && first == timed.len() {
            self.rows
                .append(&label("That was the last one today.", &["dim"]));
        }
    }

    /// The day across the drawer: overlapping events in lanes, every hour
    /// ruled, each free gap saying how long it is, the time on the now line.
    fn render_strip(&self, d: &Day) {
        let c = &self.strip;
        c.clear();
        let n = d.lanes.iter().map(|l| l.1).max().unwrap_or(1) as f64;
        let band = n * LANE_H;
        c.set_size_request(-1, (TAG_H + band + 4.0 + TICK_H) as i32);
        let step = if (d.s1 - d.s0) / 3600 > 14 { 2 } else { 1 };
        for t in d.hours() {
            c.put(
                &rule(1, -1, "grid"),
                Place {
                    x: Axis::centered(d.f(t)),
                    y: Axis::Px {
                        pos: TAG_H - 2.0,
                        size: band + 4.0,
                    },
                },
            );
            let h = local(t, d.zone).format("%-H").to_string();
            if h.parse::<i64>().unwrap_or(0) % step == 0 {
                c.put(
                    &label(&h, &["tick"]),
                    Place {
                        x: Axis::centered(d.f(t)),
                        y: Axis::Px {
                            pos: TAG_H + band + 3.0,
                            size: TICK_H,
                        },
                    },
                );
            }
        }
        for &(a, b) in &d.gaps {
            c.put(
                &d.gap(a, b),
                Place {
                    x: Axis::span(d.f(a), d.f(b), 1.0),
                    y: Axis::Px {
                        pos: TAG_H,
                        size: band,
                    },
                },
            );
        }
        for (e, &(lane, group)) in d.timed.iter().zip(&d.lanes) {
            let (a, b) = (e.start.max(d.s0), e.end.min(d.s1));
            if b <= a {
                continue;
            }
            let h = band / group as f64;
            c.put(
                &d.event(e, e.title_or_default()),
                Place {
                    x: Axis::Span {
                        from: d.f(a),
                        to: d.f(b),
                        inset: (0.0, 1.0),
                    },
                    y: Axis::Px {
                        pos: TAG_H + lane as f64 * h,
                        size: if group > 1 { h - 2.0 } else { h },
                    },
                },
            );
        }
        if d.now > d.s0 && d.now < d.s1 {
            c.put(
                &rule(2, -1, "nowline"),
                Place {
                    x: Axis::centered(d.f(d.now)),
                    y: Axis::Px {
                        pos: TAG_H - 3.0,
                        size: band + 6.0,
                    },
                },
            );
            c.put(
                &label(&hhmm(d.now, d.zone), &["nowtag"]),
                Place {
                    x: Axis::centered(d.f(d.now)),
                    y: Axis::Px {
                        pos: 0.0,
                        size: TAG_H - 2.0,
                    },
                },
            );
        }
    }

    /// The day top to bottom: tall enough to read whole titles, overlapping
    /// events side by side.
    fn render_timeline(&self, d: &Day) {
        let span_h = (d.s1 - d.s0) as f64 / 3600.0;
        let per_hour = (560.0 / span_h).clamp(24.0, 48.0);
        let height = span_h * per_hour + 2.0 * TL_PAD;
        let y = |t: i64| (TL_PAD + d.f(t) * span_h * per_hour) / height;
        let (hc, lc) = (&self.hours, &self.lanes);
        hc.clear();
        lc.clear();
        hc.set_size_request(HOURS_W, height as i32);
        lc.set_size_request(-1, height as i32);
        for t in d.hours() {
            let h = label(&local(t, d.zone).format("%-H").to_string(), &["tick"]);
            h.set_xalign(1.0);
            hc.put(
                &h,
                Place {
                    x: Axis::Span {
                        from: 0.0,
                        to: 1.0,
                        inset: (0.0, 8.0),
                    },
                    y: Axis::centered(y(t)),
                },
            );
            lc.put(
                &rule(-1, 1, "grid"),
                Place {
                    x: Axis::span(0.0, 1.0, 0.0),
                    y: Axis::centered(y(t)),
                },
            );
        }
        for &(a, b) in &d.gaps {
            let g = d.gap(a, b);
            if let Some(l) = g.first_child() {
                l.set_valign(gtk::Align::Start);
            }
            lc.put(
                &g,
                Place {
                    x: Axis::Span {
                        from: 0.0,
                        to: 1.0,
                        inset: (2.0, 6.0),
                    },
                    y: Axis::span(y(a), y(b), 1.0),
                },
            );
        }
        for (e, &(lane, group)) in d.timed.iter().zip(&d.lanes) {
            let (a, b) = (e.start.max(d.s0), e.end.min(d.s1));
            if b <= a {
                continue;
            }
            let px = (b - a) as f64 / 3600.0 * per_hour;
            let l = d.event(
                e,
                &format!("{}  {}", hhmm(e.start, d.zone), e.title_or_default()),
            );
            l.set_wrap(true);
            l.set_wrap_mode(gtk::pango::WrapMode::WordChar);
            l.set_lines((((px - 4.0) / 13.0).floor() as i32).max(1));
            l.set_yalign(0.0);
            let lanes = group as f64;
            lc.put(
                &l,
                Place {
                    x: Axis::Span {
                        from: lane as f64 / lanes,
                        to: (lane + 1) as f64 / lanes,
                        inset: (2.0, 6.0),
                    },
                    y: Axis::span(y(a), y(b), 1.0),
                },
            );
        }
        if d.now > d.s0 && d.now < d.s1 {
            lc.put(
                &rule(-1, 2, "nowline"),
                Place {
                    x: Axis::span(0.0, 1.0, 0.0),
                    y: Axis::centered(y(d.now)),
                },
            );
            hc.put(
                &label(&hhmm(d.now, d.zone), &["nowtag"]),
                Place {
                    x: Axis::At {
                        at: 1.0,
                        px: -3.0,
                        align: 1.0,
                    },
                    y: Axis::centered(y(d.now)),
                },
            );
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
