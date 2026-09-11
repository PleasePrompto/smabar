/// Rectangle in physical desktop pixels. Unlike [`super::Rect`], this is
/// never sent by the shell and may use negative multi-monitor coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScreenRect {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScreenPoint {
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HorizontalAnchor {
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerticalAnchor {
    Top,
    Bottom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlyoutDirection {
    Up,
    Down,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FlyoutPlacement {
    pub frame: ScreenRect,
    pub direction: FlyoutDirection,
    pub pointer_x: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EdgePlacement {
    pub frame: ScreenRect,
    pub direction: FlyoutDirection,
}

/// Centers a content-sized bar surface on its monitor edge. The requested
/// size includes magnification and shadow headroom.
pub fn bar_surface_frame(area: ScreenRect, size: (u32, u32), bar_at_top: bool) -> ScreenRect {
    let width = size.0.clamp(1, area.w);
    let height = size.1.clamp(1, area.h);
    let x = i64::from(area.x) + i64::from(area.w.saturating_sub(width)) / 2;
    let y = if bar_at_top {
        i64::from(area.y)
    } else {
        i64::from(area.y) + i64::from(area.h.saturating_sub(height))
    };
    ScreenRect {
        x: i32::try_from(x).unwrap_or(if x < 0 { i32::MIN } else { i32::MAX }),
        y: i32::try_from(y).unwrap_or(if y < 0 { i32::MIN } else { i32::MAX }),
        w: width,
        h: height,
    }
}

/// Moves a bar surface out through its anchored screen edge. WebKit keeps
/// painting the same static buffer while the compositor moves the native
/// window, avoiding stale transparent tiles on proprietary NVIDIA drivers.
pub fn offset_bar_surface(frame: ScreenRect, bar_at_top: bool, offset: u32) -> ScreenRect {
    let offset = i64::from(offset);
    let y = if bar_at_top {
        i64::from(frame.y) - offset
    } else {
        i64::from(frame.y) + offset
    };
    ScreenRect {
        y: i32::try_from(y).unwrap_or(if y < 0 { i32::MIN } else { i32::MAX }),
        ..frame
    }
}

/// Places the complete flyout window, including its pointer reserve, around
/// a bar tile. `gap` is measured between the tile and the flyout body.
pub fn flyout_frame(
    area: ScreenRect,
    trigger: ScreenRect,
    size: (u32, u32),
    bar_at_top: bool,
    inset: u32,
    gap: u32,
    pointer_reserve: u32,
) -> FlyoutPlacement {
    let width = size.0.min(area.w);
    let height = size.1.min(area.h);
    let area_left = i64::from(area.x);
    let area_top = i64::from(area.y);
    let area_right = area_left + i64::from(area.w);
    let area_bottom = area_top + i64::from(area.h);
    let center = i64::from(trigger.x) + i64::from(trigger.w) / 2;
    let horizontal_inset = inset.min(area.w.saturating_sub(width) / 2);
    let left = (center - i64::from(width) / 2).clamp(
        area_left + i64::from(horizontal_inset),
        area_right - i64::from(width) - i64::from(horizontal_inset),
    );
    let direction = if bar_at_top {
        FlyoutDirection::Down
    } else {
        FlyoutDirection::Up
    };
    let wanted_top = match direction {
        FlyoutDirection::Down => {
            i64::from(trigger.y) + i64::from(trigger.h) + i64::from(gap)
                - i64::from(pointer_reserve)
        }
        FlyoutDirection::Up => {
            i64::from(trigger.y) - i64::from(height) - i64::from(gap) + i64::from(pointer_reserve)
        }
    };
    let top = wanted_top.clamp(area_top, area_bottom - i64::from(height));
    let pointer_inset = pointer_reserve.min(width / 2);
    let pointer_x = (center - left).clamp(
        i64::from(pointer_inset),
        i64::from(width.saturating_sub(pointer_inset)),
    );
    FlyoutPlacement {
        frame: ScreenRect {
            x: i32::try_from(left).unwrap_or(if left < 0 { i32::MIN } else { i32::MAX }),
            y: i32::try_from(top).unwrap_or(if top < 0 { i32::MIN } else { i32::MAX }),
            w: width,
            h: height,
        },
        direction,
        pointer_x: u32::try_from(pointer_x).unwrap_or(width / 2),
    }
}

/// Places a non-interactive tooltip beside its bar trigger.
pub fn tooltip_frame(
    area: ScreenRect,
    trigger: ScreenRect,
    size: (u32, u32),
    bar_at_top: bool,
    inset: u32,
    gap: u32,
) -> EdgePlacement {
    let width = size.0.min(area.w);
    let height = size.1.min(area.h);
    let left_edge = i64::from(area.x);
    let top_edge = i64::from(area.y);
    let right_edge = left_edge + i64::from(area.w);
    let bottom_edge = top_edge + i64::from(area.h);
    let center = i64::from(trigger.x) + i64::from(trigger.w) / 2;
    let inset = inset.min(area.w.saturating_sub(width) / 2);
    let left = (center - i64::from(width) / 2).clamp(
        left_edge + i64::from(inset),
        right_edge - i64::from(width) - i64::from(inset),
    );
    let direction = if bar_at_top {
        FlyoutDirection::Down
    } else {
        FlyoutDirection::Up
    };
    let top = match direction {
        FlyoutDirection::Down => i64::from(trigger.y) + i64::from(trigger.h) + i64::from(gap),
        FlyoutDirection::Up => i64::from(trigger.y) - i64::from(height) - i64::from(gap),
    }
    .clamp(top_edge, bottom_edge - i64::from(height));
    EdgePlacement {
        frame: ScreenRect {
            x: i32::try_from(left).unwrap_or(if left < 0 { i32::MIN } else { i32::MAX }),
            y: i32::try_from(top).unwrap_or(if top < 0 { i32::MIN } else { i32::MAX }),
            w: width,
            h: height,
        },
        direction,
    }
}

/// Places a content-sized surface inside a physical monitor work area.
/// `avoid` is the bar rect when both occupy the same vertical edge.
pub fn anchored_frame(
    area: ScreenRect,
    size: (u32, u32),
    horizontal: HorizontalAnchor,
    vertical: VerticalAnchor,
    inset: u32,
    avoid: Option<ScreenRect>,
    gap: u32,
) -> ScreenRect {
    let width = size.0.min(area.w);
    let height = size.1.min(area.h);
    let inset = i64::from(inset);
    let area_left = i64::from(area.x);
    let area_top = i64::from(area.y);
    let area_right = area_left + i64::from(area.w);
    let area_bottom = area_top + i64::from(area.h);
    let width_i = i64::from(width);
    let height_i = i64::from(height);
    let x = match horizontal {
        HorizontalAnchor::Left => area_left + inset,
        HorizontalAnchor::Center => area_left + (i64::from(area.w) - width_i) / 2,
        HorizontalAnchor::Right => area_right - width_i - inset,
    }
    .clamp(area_left, area_right - width_i);
    let mut y = match vertical {
        VerticalAnchor::Top => area_top + inset,
        VerticalAnchor::Bottom => area_bottom - height_i - inset,
    };
    if let Some(bar) = avoid
        && x < i64::from(bar.x) + i64::from(bar.w)
        && x + width_i > i64::from(bar.x)
    {
        let bar_top = i64::from(bar.y);
        let bar_bottom = bar_top + i64::from(bar.h);
        y = match vertical {
            VerticalAnchor::Top => y.max(bar_bottom + i64::from(gap)),
            VerticalAnchor::Bottom => y.min(bar_top - height_i - i64::from(gap)),
        };
    }
    y = y.clamp(area_top, area_bottom - height_i);
    ScreenRect {
        x: i32::try_from(x).unwrap_or(if x < 0 { i32::MIN } else { i32::MAX }),
        y: i32::try_from(y).unwrap_or(if y < 0 { i32::MIN } else { i32::MAX }),
        w: width,
        h: height,
    }
}

pub fn union(a: ScreenRect, b: ScreenRect) -> ScreenRect {
    let left = i64::from(a.x).min(i64::from(b.x));
    let top = i64::from(a.y).min(i64::from(b.y));
    let right = (i64::from(a.x) + i64::from(a.w)).max(i64::from(b.x) + i64::from(b.w));
    let bottom = (i64::from(a.y) + i64::from(a.h)).max(i64::from(b.y) + i64::from(b.h));
    ScreenRect {
        x: i32::try_from(left).unwrap_or(i32::MIN),
        y: i32::try_from(top).unwrap_or(i32::MIN),
        w: u32::try_from(right - left).unwrap_or(u32::MAX),
        h: u32::try_from(bottom - top).unwrap_or(u32::MAX),
    }
}

/// Reserves room for a root menu and its one possible submenu, then anchors
/// that small window around the invocation point.
pub fn menu_frame(
    area: ScreenRect,
    anchor: ScreenPoint,
    panel_size: (u32, u32),
    inset: u32,
) -> ScreenRect {
    let padding = inset.saturating_mul(2);
    let width = panel_size
        .0
        .saturating_mul(2)
        .saturating_add(padding)
        .min(area.w);
    let height = panel_size.1.saturating_add(padding).min(area.h);
    let left = i64::from(area.x);
    let top = i64::from(area.y);
    let right = left + i64::from(area.w);
    let bottom = top + i64::from(area.h);
    let inset_x = inset.min(area.w.saturating_sub(width) / 2);
    let inset_y = inset.min(area.h.saturating_sub(height) / 2);
    let x = if i64::from(anchor.x) + i64::from(width) + i64::from(inset_x) <= right {
        i64::from(anchor.x)
    } else {
        i64::from(anchor.x) - i64::from(width)
    }
    .clamp(
        left + i64::from(inset_x),
        right - i64::from(width) - i64::from(inset_x),
    );
    let y = if i64::from(anchor.y) + i64::from(height) + i64::from(inset_y) <= bottom {
        i64::from(anchor.y)
    } else {
        i64::from(anchor.y) - i64::from(height)
    }
    .clamp(
        top + i64::from(inset_y),
        bottom - i64::from(height) - i64::from(inset_y),
    );
    ScreenRect {
        x: i32::try_from(x).unwrap_or(if x < 0 { i32::MIN } else { i32::MAX }),
        y: i32::try_from(y).unwrap_or(if y < 0 { i32::MIN } else { i32::MAX }),
        w: width,
        h: height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const AREA: ScreenRect = ScreenRect {
        x: -1920,
        y: 40,
        w: 1920,
        h: 1040,
    };

    #[test]
    fn places_and_clamps_on_negative_monitors() {
        assert_eq!(
            anchored_frame(
                AREA,
                (360, 200),
                HorizontalAnchor::Right,
                VerticalAnchor::Bottom,
                16,
                None,
                8,
            ),
            ScreenRect {
                x: -376,
                y: 864,
                w: 360,
                h: 200,
            }
        );
        assert_eq!(
            anchored_frame(
                AREA,
                (4000, 2000),
                HorizontalAnchor::Center,
                VerticalAnchor::Top,
                16,
                None,
                8,
            ),
            AREA
        );
    }

    #[test]
    fn centers_small_bar_surfaces_on_the_requested_edge() {
        assert_eq!(
            bar_surface_frame(AREA, (800, 120), false),
            ScreenRect {
                x: -1_360,
                y: 960,
                w: 800,
                h: 120,
            }
        );
        assert_eq!(bar_surface_frame(AREA, (4_000, 2_000), true), AREA);
    }

    #[test]
    fn moves_bar_surfaces_only_through_their_screen_edge() {
        let frame = bar_surface_frame(AREA, (800, 120), false);
        assert_eq!(offset_bar_surface(frame, false, 112).y, 1_072);
        assert_eq!(offset_bar_surface(frame, true, 112).y, 848);
        // A system panel may sit between the bar and the physical screen edge.
        assert_eq!(offset_bar_surface(frame, false, 160).y, 1_120);
        assert_eq!(offset_bar_surface(frame, true, 160).y, 800);
        assert_eq!(offset_bar_surface(frame, false, u32::MAX).y, i32::MAX);
    }

    #[test]
    fn avoids_bar_on_the_same_edge() {
        let bar = ScreenRect {
            x: -1500,
            y: 1000,
            w: 800,
            h: 60,
        };
        let frame = anchored_frame(
            AREA,
            (360, 200),
            HorizontalAnchor::Center,
            VerticalAnchor::Bottom,
            16,
            Some(bar),
            8,
        );
        assert_eq!(frame.y, 792);
    }

    #[test]
    fn unions_distant_notification_surfaces() {
        assert_eq!(
            union(
                ScreenRect {
                    x: 10,
                    y: 20,
                    w: 100,
                    h: 50,
                },
                ScreenRect {
                    x: 200,
                    y: 5,
                    w: 20,
                    h: 30,
                },
            ),
            ScreenRect {
                x: 10,
                y: 5,
                w: 210,
                h: 65,
            }
        );
    }

    #[test]
    fn flyout_tracks_the_tile_and_dock_edge() {
        let trigger = ScreenRect {
            x: -1100,
            y: 990,
            w: 48,
            h: 48,
        };
        let placed = flyout_frame(AREA, trigger, (340, 400), false, 12, 12, 8);
        assert_eq!(placed.direction, FlyoutDirection::Up);
        assert_eq!(placed.frame.y, 586);
        assert_eq!(placed.pointer_x, 170);

        let top = flyout_frame(AREA, trigger, (4000, 400), true, 12, 12, 8);
        assert_eq!(top.direction, FlyoutDirection::Down);
        assert_eq!(top.frame.w, AREA.w);
        assert_eq!(top.frame.x, AREA.x);
    }

    #[test]
    fn tooltip_tracks_the_tile_without_leaving_the_monitor() {
        let trigger = ScreenRect {
            x: -1910,
            y: 990,
            w: 40,
            h: 40,
        };
        let placed = tooltip_frame(AREA, trigger, (120, 28), false, 8, 8);
        assert_eq!(placed.direction, FlyoutDirection::Up);
        assert_eq!(placed.frame.x, -1912);
        assert_eq!(placed.frame.y, 954);
    }

    #[test]
    fn menu_reserves_one_submenu_and_flips_at_edges() {
        assert_eq!(
            menu_frame(AREA, ScreenPoint { x: -20, y: 1060 }, (280, 300), 8,),
            ScreenRect {
                x: -596,
                y: 744,
                w: 576,
                h: 316,
            }
        );
    }

    #[test]
    fn menu_surface_reserves_the_shell_inset_around_its_panels() {
        let frame = menu_frame(AREA, ScreenPoint { x: -960, y: 40 }, (280, 300), 8);
        assert_eq!((frame.w, frame.h), (576, 316));
    }
}
