use std::sync::atomic::{AtomicBool, Ordering};

use super::*;

struct Cleanup(Arc<AtomicBool>);

impl Drop for Cleanup {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

#[tokio::test]
async fn a_timed_out_stop_waits_for_task_cleanup() {
    let cleaned = Arc::new(AtomicBool::new(false));
    let task_cleaned = Arc::clone(&cleaned);
    let (started, ready) = oneshot::channel();
    let task = tokio::spawn(async move {
        let _cleanup = Cleanup(task_cleaned);
        let _ = started.send(());
        std::future::pending::<()>().await;
    });
    ready.await.expect("task started");
    let (commands, _rx) = mpsc::channel(1);
    let manifest = serde_json::from_value(serde_json::json!({
        "id": "stubborn",
        "name": "Stubborn",
        "version": "1.0.0",
        "protocolVersion": 1,
        "runtime": "exec",
        "command": ["stubborn"],
        "tiles": [{ "id": "w", "name": "W" }]
    }))
    .expect("valid manifest");
    stop_handle_with_timeout(
        PluginHandle {
            dir: PathBuf::from("/stubborn"),
            manifest,
            commands,
            task,
        },
        Duration::ZERO,
    )
    .await;

    assert!(cleaned.load(Ordering::SeqCst));
}

#[tokio::test]
async fn a_full_command_queue_rejects_actions_instead_of_waiting() {
    let (_dir, paths) = crate::plugins::tests::temp_paths();
    let config = Arc::new(ConfigWatcher::spawn(paths.clone()).expect("config watcher"));
    let supervisor = PluginSupervisor::start(
        paths,
        ProviderHub::new(),
        config,
        SupervisorOptions::default(),
    )
    .await;
    let (commands, _receiver) = mpsc::channel(1);
    commands
        .try_send(PluginCommand::Shutdown)
        .expect("fill command queue");
    let manifest = serde_json::from_value(serde_json::json!({
        "id": "busy",
        "name": "Busy",
        "version": "1.0.0",
        "protocolVersion": 1,
        "runtime": "exec",
        "command": ["busy"],
        "tiles": [{ "id": "w", "name": "W" }]
    }))
    .expect("valid manifest");
    let task = tokio::spawn(std::future::pending());
    lock_unpoisoned(&supervisor.inner.plugins).insert(
        "busy".to_string(),
        PluginHandle {
            dir: PathBuf::from("/busy"),
            manifest,
            commands,
            task,
        },
    );

    assert!(matches!(
        supervisor.dispatch_action("busy", "w", "open", None).await,
        Err(PluginError::Busy { .. })
    ));

    let handle = lock_unpoisoned(&supervisor.inner.plugins)
        .remove("busy")
        .expect("fake handle");
    handle.task.abort();
    supervisor.shutdown_all().await;
}

#[tokio::test]
async fn shutdown_all_is_idempotent_for_the_exit_fallback() {
    let (_dir, paths) = crate::plugins::tests::temp_paths();
    let config = Arc::new(ConfigWatcher::spawn(paths.clone()).expect("config watcher"));
    let supervisor = PluginSupervisor::start(
        paths,
        ProviderHub::new(),
        config,
        SupervisorOptions::default(),
    )
    .await;
    // The tray quit stops plugins on a live loop; the exit callback repeats
    // the call on a dead loop and must find nothing left to wait for.
    supervisor.shutdown_all().await;
    tokio::time::timeout(Duration::from_secs(1), supervisor.shutdown_all())
        .await
        .expect("second shutdown returns at once");
    assert!(lock_unpoisoned(&supervisor.inner.plugins).is_empty());
}
