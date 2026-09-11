//! Behavior of the screenshot tools against a stand-in bar.

use base64::Engine as _;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::ContentBlock;

use crate::capture::UiAction;

use super::tests::{ONE_PIXEL_PNG, test_handler};
use super::types::{BarScreenshotParams, BarUiStateParams};

#[tokio::test]
async fn returns_the_picture_as_an_image_block() {
    let (_dir, handler) = test_handler().await;
    let result = handler
        .bar_screenshot(Parameters(BarScreenshotParams {
            target: None,
            scale: None,
        }))
        .await
        .expect("screenshot");
    let [ContentBlock::Image(image), ContentBlock::Text(text)] = result.content.as_slice() else {
        panic!(
            "expected an image and a text block, got {:?}",
            result.content
        );
    };
    assert_eq!(image.mime_type, "image/png");
    assert_eq!(
        base64::engine::general_purpose::STANDARD
            .decode(&image.data)
            .expect("valid base64"),
        ONE_PIXEL_PNG
    );
    assert!(text.text.contains("target: bar"), "{}", text.text);
    assert!(text.text.contains("120x40"), "{}", text.text);
}

#[tokio::test]
async fn magnifies_up_to_the_cap() {
    let (_dir, handler) = test_handler().await;
    let result = handler
        .bar_screenshot(Parameters(BarScreenshotParams {
            target: Some("plugin:clock:clock".into()),
            // Above the cap: clamped to 4 rather than refused.
            scale: Some(9),
        }))
        .await
        .expect("screenshot");
    let ContentBlock::Text(text) = &result.content[1] else {
        panic!("expected a text block");
    };
    assert!(text.text.contains("png: 480x160"), "{}", text.text);
}

#[tokio::test]
async fn an_unknown_target_answers_with_the_ones_that_exist() {
    let (_dir, handler) = test_handler().await;
    let error = handler
        .bar_screenshot(Parameters(BarScreenshotParams {
            target: Some("plugin:nope:nope".into()),
            scale: None,
        }))
        .await
        .expect_err("unknown target");
    let message = error.to_string();
    assert!(message.contains("plugin:nope:nope"), "{message}");
    assert!(message.contains("bar, plugin:clock:clock"), "{message}");
}

#[tokio::test]
async fn opening_a_flyout_without_a_tile_is_refused_before_it_reaches_the_bar() {
    let (_dir, handler) = test_handler().await;
    let error = handler
        .bar_ui_state(Parameters(BarUiStateParams {
            action: UiAction::OpenFlyout,
            tile_id: None,
            group: None,
        }))
        .await
        .expect_err("missing tile id");
    assert!(error.to_string().contains("tileId"), "{error}");
}

#[tokio::test]
async fn closing_a_surface_needs_no_tile() {
    let (_dir, handler) = test_handler().await;
    let result = handler
        .bar_ui_state(Parameters(BarUiStateParams {
            action: UiAction::CloseFlyout,
            tile_id: None,
            group: None,
        }))
        .await
        .expect("close");
    let ContentBlock::Text(text) = &result.content[0] else {
        panic!("expected a text block");
    };
    assert!(
        text.text.contains("bar, plugin:clock:clock"),
        "{}",
        text.text
    );
}
