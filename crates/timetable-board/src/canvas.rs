//! `Canvas`: a widget whose children sit at fractions of its own size. The
//! day strip and the day timeline place events by time on it, so they fill
//! whatever width (or height) the drawer gives them.

use gtk::glib;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use std::cell::RefCell;

/// Where a child sits along one axis.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Axis {
    /// From one fraction of the length to another, inset by pixels at the
    /// start and the end.
    Span {
        from: f64,
        to: f64,
        inset: (f64, f64),
    },
    /// The child's natural size at a fraction plus pixels, moved back by
    /// `align` of its size (0.5 centers it) and kept inside.
    At { at: f64, px: f64, align: f64 },
    /// Fixed pixels.
    Px { pos: f64, size: f64 },
}

impl Axis {
    pub fn span(from: f64, to: f64, inset: f64) -> Axis {
        Axis::Span {
            from,
            to,
            inset: (inset, inset),
        }
    }

    pub fn centered(at: f64) -> Axis {
        Axis::At {
            at,
            px: 0.0,
            align: 0.5,
        }
    }

    /// Position and size along a length, for a child of natural size `nat`.
    pub fn resolve(self, len: f64, nat: f64) -> (f64, f64) {
        match self {
            Axis::Span { from, to, inset } => (
                from * len + inset.0,
                ((to - from) * len - inset.0 - inset.1).max(0.0),
            ),
            Axis::At { at, px, align } => (
                (at * len + px - align * nat).clamp(0.0, (len - nat).max(0.0)),
                nat,
            ),
            Axis::Px { pos, size } => (pos, size),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Place {
    pub x: Axis,
    pub y: Axis,
}

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct Canvas {
        pub kids: RefCell<Vec<(gtk::Widget, Place)>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Canvas {
        const NAME: &'static str = "TtCanvas";
        type Type = super::Canvas;
        type ParentType = gtk::Widget;
    }

    impl ObjectImpl for Canvas {
        fn dispose(&self) {
            for (c, _) in self.kids.borrow_mut().drain(..) {
                c.unparent();
            }
        }
    }

    impl WidgetImpl for Canvas {
        // Takes the size it is given; a size request sets its height.
        fn measure(&self, _o: gtk::Orientation, _for_size: i32) -> (i32, i32, i32, i32) {
            (0, 0, -1, -1)
        }

        fn size_allocate(&self, width: i32, height: i32, _baseline: i32) {
            for (c, p) in self.kids.borrow().iter() {
                if !c.should_layout() {
                    continue;
                }
                let (_, nat_w, _, _) = c.measure(gtk::Orientation::Horizontal, -1);
                let (x, w) = p.x.resolve(width as f64, nat_w as f64);
                let w = w.round() as i32;
                let (_, nat_h, _, _) = c.measure(gtk::Orientation::Vertical, w);
                let (y, h) = p.y.resolve(height as f64, nat_h as f64);
                c.size_allocate(
                    &gtk::Allocation::new(x.round() as i32, y.round() as i32, w, h.round() as i32),
                    -1,
                );
            }
        }
    }
}

glib::wrapper! {
    pub struct Canvas(ObjectSubclass<imp::Canvas>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for Canvas {
    fn default() -> Self {
        Self::new()
    }
}

impl Canvas {
    pub fn new() -> Canvas {
        let c: Canvas = glib::Object::new();
        c.set_overflow(gtk::Overflow::Hidden);
        c
    }

    pub fn put(&self, w: &impl IsA<gtk::Widget>, place: Place) {
        w.set_parent(self);
        self.imp()
            .kids
            .borrow_mut()
            .push((w.clone().upcast(), place));
        self.queue_allocate();
    }

    pub fn clear(&self) {
        for (c, _) in self.imp().kids.borrow_mut().drain(..) {
            c.unparent();
        }
        self.queue_allocate();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn axes() {
        assert_eq!(
            Axis::span(0.25, 0.5, 1.0).resolve(400.0, 0.0),
            (101.0, 98.0)
        );
        // Centered on the point, kept inside at the edges.
        assert_eq!(Axis::centered(0.5).resolve(400.0, 20.0), (190.0, 20.0));
        assert_eq!(Axis::centered(0.0).resolve(400.0, 20.0), (0.0, 20.0));
        assert_eq!(Axis::centered(1.0).resolve(400.0, 20.0), (380.0, 20.0));
        assert_eq!(
            Axis::Px {
                pos: 3.0,
                size: 9.0
            }
            .resolve(400.0, 50.0),
            (3.0, 9.0)
        );
        // Too narrow is empty, never negative.
        assert_eq!(Axis::span(0.5, 0.5, 1.0).resolve(400.0, 0.0).1, 0.0);
    }
}
