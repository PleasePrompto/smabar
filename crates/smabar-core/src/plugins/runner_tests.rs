use super::*;

#[tokio::test]
async fn backoff_rejects_instead_of_acknowledging_an_action() {
    let (commands, mut receiver) = mpsc::channel(2);
    let (accepted, result) = oneshot::channel();
    commands
        .send(PluginCommand::Action {
            tile_id: "status".into(),
            action: "refresh".into(),
            value: None,
            accepted,
        })
        .await
        .expect("queue action");
    commands
        .send(PluginCommand::Shutdown)
        .await
        .expect("queue shutdown");

    assert!(!backoff_wait(&mut receiver, Duration::from_secs(60)).await);
    assert!(
        result.await.is_err(),
        "a down process must not accept actions"
    );
}
