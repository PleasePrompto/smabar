//! Where the settings window opens: a remembered spot while it is still
//! reachable, the middle of the work area otherwise.

use super::surfaces::{ScreenPoint, ScreenRect};

/// Width of the settings window that must stay on the monitor for a
/// remembered position to be reused, and the title-bar strip that must be
/// fully visible so the window can still be grabbed.
const SETTINGS_MIN_VISIBLE_WIDTH: u32 = 64;
const SETTINGS_TITLE_STRIP: u32 = 32;

/// Where the settings window opens, in physical pixels: the remembered
/// top-left corner when enough of the window stays reachable inside the
/// work area, otherwise centered. A monitor that went away or a resolution
/// change therefore never strands the window off-screen.
pub fn settings_origin(
    remembered: Option<ScreenPoint>,
    size: (u32, u32),
    area: ScreenRect,
) -> ScreenPoint {
    let width = i64::from(size.0);
    let height = i64::from(size.1);
    let area_left = i64::from(area.x);
    let area_top = i64::from(area.y);
    let area_right = area_left + i64::from(area.w);
    let area_bottom = area_top + i64::from(area.h);
    if let Some(point) = remembered {
        let x = i64::from(point.x);
        let y = i64::from(point.y);
        let visible = i64::from(SETTINGS_MIN_VISIBLE_WIDTH.min(size.0));
        let reachable = x + visible <= area_right
            && x + width - visible >= area_left
            && y >= area_top
            && y + i64::from(SETTINGS_TITLE_STRIP.min(size.1)) <= area_bottom;
        if reachable {
            return point;
        }
    }
    let x = area_left + (i64::from(area.w) - width).max(0) / 2;
    let y = area_top + (i64::from(area.h) - height).max(0) / 2;
    ScreenPoint {
        x: i32::try_from(x).unwrap_or(if x < 0 { i32::MIN } else { i32::MAX }),
        y: i32::try_from(y).unwrap_or(if y < 0 { i32::MIN } else { i32::MAX }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_origin_reuses_reachable_positions_and_centers_the_rest() {
        let area = ScreenRect {
            x: 0,
            y: 0,
            w: 2560,
            h: 1600,
        };
        let size = (960, 680);
        let inside = ScreenPoint { x: 400, y: 300 };
        assert_eq!(settings_origin(Some(inside), size, area), inside);
        // Mostly off the right edge, but a 64 px strip stays grabbable.
        let edge = ScreenPoint {
            x: 2560 - 64,
            y: 10,
        };
        assert_eq!(settings_origin(Some(edge), size, area), edge);
        let centered = ScreenPoint { x: 800, y: 460 };
        assert_eq!(settings_origin(None, size, area), centered);
        // Too far right, title bar above the top, title bar below the bottom.
        for stranded in [
            ScreenPoint {
                x: 2560 - 63,
                y: 10,
            },
            ScreenPoint { x: 400, y: -1 },
            ScreenPoint {
                x: 400,
                y: 1600 - 31,
            },
        ] {
            assert_eq!(settings_origin(Some(stranded), size, area), centered);
        }
        // A window larger than the work area sits at the area origin.
        assert_eq!(
            settings_origin(None, (4000, 3000), area),
            ScreenPoint { x: 0, y: 0 }
        );
    }
}
