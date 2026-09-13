//! Pure Windows coordinate math, also compiled by Linux unit tests.

use smabar_core::platform::Rect;

const DEFAULT_DPI: f64 = 96.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DockEdge {
    Top,
    Bottom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PhysicalRect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl PhysicalRect {
    pub fn contains(self, x: i32, y: i32) -> bool {
        x >= self.left && x < self.right && y >= self.top && y < self.bottom
    }
}

pub fn scale_for_dpi(dpi: u32) -> Option<f64> {
    (dpi > 0).then(|| f64::from(dpi) / DEFAULT_DPI)
}

pub fn physical_length(logical: f64, scale: f64, maximum: i32) -> Option<i32> {
    if !logical.is_finite() || logical <= 0.0 || !scale.is_finite() || scale <= 0.0 || maximum <= 0
    {
        return None;
    }
    Some((logical * scale).round().clamp(1.0, f64::from(maximum)) as i32)
}

pub fn scale_rect(rect: Rect, scale: f64) -> Option<PhysicalRect> {
    if !scale.is_finite() || scale <= 0.0 {
        return None;
    }
    let left = (f64::from(rect.x) * scale).floor() as i32;
    let top = (f64::from(rect.y) * scale).floor() as i32;
    let right = ((f64::from(rect.x) + f64::from(rect.w)) * scale).ceil() as i32;
    let bottom = ((f64::from(rect.y) + f64::from(rect.h)) * scale).ceil() as i32;
    Some(PhysicalRect {
        left,
        top,
        right,
        bottom,
    })
}

pub fn hits_rect(rects: &[Rect], x: i32, y: i32, scale: f64) -> bool {
    rects
        .iter()
        .filter_map(|rect| scale_rect(*rect, scale))
        .any(|rect| rect.contains(x, y))
}

/// Physical thickness the bar reserves at `edge`, or `None` while the bar
/// rect does not reach into its window — the shell's first report still
/// carries the bootstrap frame, so this is a normal transient state, not
/// an error.
pub fn reservation_thickness(
    edge: DockEdge,
    bar: Rect,
    client_height: i32,
    scale: f64,
) -> Option<i32> {
    if bar.w == 0 || bar.h == 0 || client_height <= 0 {
        return None;
    }
    let bar = scale_rect(bar, scale)?;
    let thickness = match edge {
        DockEdge::Top => bar.bottom,
        DockEdge::Bottom => client_height.saturating_sub(bar.top),
    };
    (thickness > 0).then_some(thickness.min(client_height))
}

pub fn fit_appbar(edge: DockEdge, queried: PhysicalRect, thickness: i32) -> PhysicalRect {
    match edge {
        DockEdge::Top => PhysicalRect {
            bottom: queried.top.saturating_add(thickness),
            ..queried
        },
        DockEdge::Bottom => PhysicalRect {
            top: queried.bottom.saturating_sub(thickness),
            ..queried
        },
    }
}

/// What an applied reservation committed: edge, physical thickness and the
/// rect requested from the shell.
pub type ReservationKey = (DockEdge, i32, PhysicalRect);

/// The rect to commit after an AppBar notification, or `None` when the shell's
/// answer already matches the applied reservation: committing it again would
/// echo as another notification and keep the bar moving.
pub fn reassert_rect(
    applied: ReservationKey,
    edge: DockEdge,
    queried: PhysicalRect,
    thickness: i32,
) -> Option<PhysicalRect> {
    let requested = fit_appbar(edge, queried, thickness);
    (applied != (edge, thickness, requested)).then_some(requested)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scales_css_bounds_outward_and_hit_tests_half_open_edges() {
        let rect = Rect {
            x: -1,
            y: 3,
            w: 10,
            h: 5,
        };
        assert_eq!(
            scale_rect(rect, 1.25),
            Some(PhysicalRect {
                left: -2,
                top: 3,
                right: 12,
                bottom: 10,
            })
        );
        assert!(hits_rect(&[rect], -2, 3, 1.25));
        assert!(hits_rect(&[rect], 11, 9, 1.25));
        assert!(!hits_rect(&[rect], 12, 9, 1.25));
        assert!(!hits_rect(&[rect], 11, 10, 1.25));
        assert_eq!(scale_rect(rect, 0.0), None);
    }

    #[test]
    fn derives_edge_thickness_and_preserves_it_after_query() {
        let top = Rect {
            x: 200,
            y: 8,
            w: 800,
            h: 40,
        };
        let bottom = Rect {
            x: 200,
            y: 712,
            w: 800,
            h: 40,
        };
        assert_eq!(
            reservation_thickness(DockEdge::Top, top, 800, 1.25),
            Some(60)
        );
        assert_eq!(
            reservation_thickness(DockEdge::Bottom, bottom, 1_000, 1.25),
            Some(110)
        );

        let queried = PhysicalRect {
            left: -1_920,
            top: 24,
            right: 0,
            bottom: 1_080,
        };
        assert_eq!(
            fit_appbar(DockEdge::Top, queried, 60),
            PhysicalRect {
                bottom: 84,
                ..queried
            }
        );
        assert_eq!(
            fit_appbar(DockEdge::Bottom, queried, 60),
            PhysicalRect {
                top: 1_020,
                ..queried
            }
        );
    }

    #[test]
    fn notifications_recommit_only_a_changed_reservation() {
        let queried = PhysicalRect {
            left: 0,
            top: 0,
            right: 1_920,
            bottom: 1_080,
        };
        let applied = (
            DockEdge::Bottom,
            60,
            fit_appbar(DockEdge::Bottom, queried, 60),
        );
        assert_eq!(reassert_rect(applied, DockEdge::Bottom, queried, 60), None);
        assert_eq!(
            reassert_rect(applied, DockEdge::Bottom, queried, 72),
            Some(fit_appbar(DockEdge::Bottom, queried, 72))
        );
        let shifted = PhysicalRect {
            bottom: 1_040,
            ..queried
        };
        assert_eq!(
            reassert_rect(applied, DockEdge::Bottom, shifted, 60),
            Some(fit_appbar(DockEdge::Bottom, shifted, 60))
        );
        assert_eq!(
            reassert_rect(applied, DockEdge::Top, queried, 60),
            Some(fit_appbar(DockEdge::Top, queried, 60))
        );
    }

    #[test]
    fn bootstrap_rect_outside_the_window_reserves_nothing() {
        // The shell's first report still carries the bootstrap frame: a bar
        // rect below a viewport that is not tall enough to contain it yet.
        let bootstrap = Rect {
            x: 0,
            y: 196,
            w: 800,
            h: 48,
        };
        assert_eq!(
            reservation_thickness(DockEdge::Bottom, bootstrap, 77, 1.0),
            None
        );
        let above = Rect {
            x: 0,
            y: -60,
            w: 800,
            h: 48,
        };
        assert_eq!(reservation_thickness(DockEdge::Top, above, 800, 1.0), None);
        let taller_than_window = Rect {
            x: 0,
            y: 0,
            w: 800,
            h: 120,
        };
        assert_eq!(
            reservation_thickness(DockEdge::Top, taller_than_window, 77, 1.0),
            Some(77)
        );
    }

    #[test]
    fn converts_dpi_to_css_scale() {
        assert_eq!(scale_for_dpi(96), Some(1.0));
        assert_eq!(scale_for_dpi(120), Some(1.25));
        assert_eq!(scale_for_dpi(0), None);
        assert_eq!(physical_length(48.0, 1.5, 1_000), Some(72));
        assert_eq!(physical_length(48.0, 1.5, 60), Some(60));
        assert_eq!(physical_length(0.0, 1.5, 1_000), None);
    }
}
