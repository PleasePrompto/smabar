//! macOS Accessibility uses global logical points, with a top-left origin.

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Frame {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Frame {
    pub fn valid(self) -> bool {
        [self.x, self.y, self.w, self.h]
            .iter()
            .all(|value| value.is_finite())
            && self.w > 0.0
            && self.h > 0.0
    }

    pub fn same(self, other: Self) -> bool {
        [
            self.x - other.x,
            self.y - other.y,
            self.w - other.w,
            self.h - other.h,
        ]
        .iter()
        .all(|difference| difference.abs() < 0.5)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Area {
    pub screen: Frame,
    pub usable: Frame,
}

impl Area {
    /// Only ordinary windows whose center belongs to this screen are adjusted.
    /// The caller excludes fullscreen/minimized windows before calling this.
    pub fn constrain(self, window: Frame) -> Option<Frame> {
        if !window.valid() || !self.screen.valid() || !self.usable.valid() {
            return None;
        }
        let cx = window.x + window.w / 2.0;
        let cy = window.y + window.h / 2.0;
        if cx < self.screen.x
            || cx >= self.screen.x + self.screen.w
            || cy < self.screen.y
            || cy >= self.screen.y + self.screen.h
        {
            return None;
        }
        let height = window.h.min(self.usable.h);
        let y = window
            .y
            .clamp(self.usable.y, self.usable.y + self.usable.h - height);
        let adjusted = Frame {
            y,
            h: height,
            ..window
        };
        (!window.same(adjusted)).then_some(adjusted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reserve_moves_small_windows_and_shrinks_expanded_windows_on_either_edge() {
        let screen = Frame {
            x: 0.0,
            y: 0.0,
            w: 1280.0,
            h: 800.0,
        };
        let full = Frame {
            y: 25.0,
            h: 700.0,
            ..screen
        };
        let top = Area {
            screen,
            usable: Frame {
                y: 85.0,
                h: 640.0,
                ..screen
            },
        };
        assert_eq!(
            top.constrain(full),
            Some(Frame {
                y: 85.0,
                h: 640.0,
                ..full
            })
        );
        let small = Frame {
            x: 200.0,
            y: 30.0,
            w: 500.0,
            h: 300.0,
        };
        assert_eq!(top.constrain(small), Some(Frame { y: 85.0, ..small }));
        let bottom = Area {
            screen,
            usable: Frame {
                y: 25.0,
                h: 640.0,
                ..screen
            },
        };
        assert_eq!(bottom.constrain(full), Some(Frame { h: 640.0, ..full }));
        let fitted = Frame { y: 100.0, ..small };
        assert_eq!(top.constrain(fitted), None);
        assert_eq!(bottom.constrain(fitted), None);
    }

    #[test]
    fn reserve_preserves_other_screens_fractional_points_and_user_moved_windows() {
        let screen = Frame {
            x: -1440.0,
            y: -200.0,
            w: 1440.0,
            h: 900.0,
        };
        let area = Area {
            screen,
            usable: Frame {
                y: -119.5,
                h: 739.5,
                ..screen
            },
        };
        let other = Frame {
            x: 0.0,
            y: 0.0,
            w: 900.0,
            h: 650.0,
        };
        assert_eq!(area.constrain(other), None);
        let local = Frame {
            x: -1400.0,
            y: -150.0,
            w: 800.0,
            h: 500.0,
        };
        let fitted = area.constrain(local).expect("window overlaps reservation");
        assert_eq!(fitted.y, -119.5);
        assert_eq!(fitted.h, 500.0);
        assert!(!fitted.same(Frame {
            y: fitted.y + 5.0,
            ..fitted
        }));
        assert_eq!(
            area.constrain(Frame {
                h: f64::NAN,
                ..local
            }),
            None
        );
        assert_eq!(
            Area {
                usable: Frame { h: 0.0, ..screen },
                ..area
            }
            .constrain(local),
            None
        );
    }
}
