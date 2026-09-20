use smabar_core::platform::surfaces::ScreenRect;

use super::super::focus::{dismisses_on_focus_loss, focus_loss_is_stale};
use super::{ActiveFlyout, FlyoutMode, OverlayFlyoutRequest, peek_to_promote};

#[test]
fn focus_loss_only_dismisses_interactive_overlays() {
    assert!(dismisses_on_focus_loss(false, Some(FlyoutMode::Pinned)));
    assert!(dismisses_on_focus_loss(true, Some(FlyoutMode::Peek)));
    assert!(!dismisses_on_focus_loss(false, Some(FlyoutMode::Peek)));
    assert!(!dismisses_on_focus_loss(false, None));
}

#[test]
fn focus_loss_older_than_the_newest_content_does_not_dismiss_it() {
    // A click on the next trigger: its open (gen 7) beat the overlay's
    // `Focused(false)`, which belongs to the state focused at gen 6.
    assert!(focus_loss_is_stale(Some(7), None, 6));
    // Ordinary dismissal: the content the user focused is clicked away.
    assert!(!focus_loss_is_stale(Some(6), None, 6));
    assert!(!focus_loss_is_stale(Some(5), Some(6), 6));
    // The newest of several open surfaces decides.
    assert!(!focus_loss_is_stale(None, None, 6));
}

#[test]
fn clicking_an_existing_peek_promotes_the_same_surface() {
    let active = ActiveFlyout {
        request: OverlayFlyoutRequest {
            generation: 7,
            tile_id: "clock".to_string(),
            mode: FlyoutMode::Peek,
            preserve_content: false,
        },
        trigger: ScreenRect {
            x: 10,
            y: 20,
            w: 30,
            h: 40,
        },
    };

    assert_eq!(
        peek_to_promote(Some(&active), "clock", FlyoutMode::Pinned),
        Some(7)
    );
    assert_eq!(
        peek_to_promote(Some(&active), "weather", FlyoutMode::Pinned),
        None
    );
}
