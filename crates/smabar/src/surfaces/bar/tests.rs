use super::*;

#[test]
fn native_motion_keeps_the_activation_strip_visible() {
    let screen = ScreenRect {
        x: -1280,
        y: -800,
        w: 1280,
        h: 800,
    };
    // The bar starts inside the work area, below a 26px top panel or above
    // a 48px bottom panel. Only 8px may remain at the physical screen edge.
    for (position, frame, expected_y) in [
        (
            BarPosition::Top,
            ScreenRect {
                y: -774,
                h: 68,
                ..screen
            },
            -860,
        ),
        (
            BarPosition::Bottom,
            ScreenRect {
                y: -116,
                h: 68,
                ..screen
            },
            -8,
        ),
    ] {
        let offset = target_offset(
            frame,
            screen,
            position,
            1.0,
            LayoutBehavior::Autohide,
            false,
            8,
        );
        assert_eq!(
            offset_bar_surface(frame, position == BarPosition::Top, offset).y,
            expected_y
        );
        assert_eq!(
            target_offset(
                frame,
                screen,
                position,
                1.0,
                LayoutBehavior::Float,
                false,
                8
            ),
            0
        );
        assert_eq!(
            target_offset(
                frame,
                screen,
                position,
                1.0,
                LayoutBehavior::Autohide,
                true,
                8
            ),
            0
        );
    }
    let scaled = ScreenRect {
        x: 0,
        y: 0,
        w: 2560,
        h: 1600,
    };
    let frame = ScreenRect {
        y: 1368,
        h: 136,
        ..scaled
    };
    let offset = target_offset(
        frame,
        scaled,
        BarPosition::Bottom,
        2.0,
        LayoutBehavior::Autohide,
        false,
        8,
    );
    assert_eq!(offset_bar_surface(frame, false, offset).y, 1584);
    assert_eq!(motion_offset(0, 60, 180, 180), 60);
    assert!(motion_offset(0, 60, 90, 180) > 30);
}

#[test]
fn cached_geometry_is_reused_only_at_its_measured_edge() {
    let manager = SurfaceManager::new(None, String::new());
    let rect = Rect {
        x: 10,
        y: 20,
        w: 800,
        h: 48,
    };
    manager.lifecycle.lock().expect("lifecycle").bar_geometry = Some((BarPosition::Bottom, rect));

    assert_eq!(
        manager.bar_geometry(BarPosition::Bottom).expect("geometry"),
        Some(rect)
    );
    assert_eq!(
        manager.bar_geometry(BarPosition::Top).expect("geometry"),
        None
    );
}

#[test]
fn edge_change_stays_pending_until_placement_succeeds() {
    let manager = SurfaceManager::new(None, String::new());
    manager.lifecycle.lock().expect("lifecycle").bar_relocation = Some(BarPosition::Top);

    let stale = manager
        .begin_bar_relocation_frame(BarPosition::Top)
        .expect("first frame")
        .expect("pending relocation");
    let current = manager
        .begin_bar_relocation_frame(BarPosition::Top)
        .expect("second frame")
        .expect("pending relocation");
    assert!(
        !manager
            .claim_bar_relocation_frame(BarPosition::Top, stale)
            .expect("stale frame")
    );
    assert!(
        manager
            .claim_bar_relocation_frame(BarPosition::Top, current)
            .expect("current frame")
    );
    assert_eq!(
        manager
            .begin_bar_relocation_frame(BarPosition::Top)
            .expect("finished relocation"),
        None
    );
}

#[test]
fn bar_rect_uses_the_new_native_frame_during_an_edge_change() {
    let rect = Rect {
        x: 0,
        y: 22,
        w: 2_560,
        h: 46,
    };

    assert_eq!(
        bar_rect_in_frame(
            ScreenRect {
                x: 0,
                y: 2_984,
                w: 2_560,
                h: 68,
            },
            rect,
            1.0,
        ),
        ScreenRect {
            x: 0,
            y: 3_006,
            w: 2_560,
            h: 46,
        }
    );
}

#[test]
fn only_the_newest_deferred_reservation_is_current() {
    let manager = SurfaceManager::new(None, String::new());
    let first = manager.next_bar_reservation().unwrap();
    let second = manager.next_bar_reservation().unwrap();
    assert!(!manager.bar_reservation_is_current(first).unwrap());
    assert!(manager.bar_reservation_is_current(second).unwrap());
}
