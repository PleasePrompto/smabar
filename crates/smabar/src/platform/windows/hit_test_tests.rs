use std::{sync::mpsc, thread, time::Duration};

use windows::Win32::{
    Foundation::{HWND, LPARAM, WPARAM},
    UI::WindowsAndMessaging::{SMTO_ABORTIFHUNG, SendMessageTimeoutW, WM_NULL},
};

use super::{OWNED_STYLE_BITS, desired_style, raw_hwnd, restored_style};

#[test]
fn style_changes_preserve_foreign_bits_and_restore_owned_bits() {
    let foreign = 0x0040_0100;
    let transparent = desired_style(foreign, true);
    assert_eq!(transparent & !OWNED_STYLE_BITS, foreign);
    assert_ne!(transparent & OWNED_STYLE_BITS, 0);

    let solid = desired_style(transparent, false);
    assert_eq!(solid & !OWNED_STYLE_BITS, foreign);
    assert_eq!(restored_style(solid, 0), foreign);
    assert_eq!(restored_style(foreign, OWNED_STYLE_BITS), transparent);
}

#[test]
fn hook_wait_pumps_cross_thread_sent_messages() {
    let hwnd = super::super::proxy::create().expect("create message target");
    let raw = raw_hwnd(hwnd);
    let (result_tx, result_rx) = mpsc::sync_channel(1);
    let sender = thread::spawn(move || {
        let completed = unsafe {
            SendMessageTimeoutW(
                HWND(raw as _),
                WM_NULL,
                WPARAM(0),
                LPARAM(0),
                SMTO_ABORTIFHUNG,
                1_000,
                None,
            )
        }
        .0 != 0;
        result_tx.send(completed).expect("report send result");
    });

    let wait = super::hook::wait_for_hook(&sender);
    let completed = result_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("receive send result");
    let joined = sender.join();
    let destroyed = super::super::proxy::destroy(hwnd);

    wait.expect("wait for sender");
    joined.expect("sender thread");
    destroyed.expect("destroy message target");
    assert!(completed, "sent message timed out instead of being pumped");
}
