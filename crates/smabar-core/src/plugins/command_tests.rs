//! End-to-end calls over the real plugin transport, including a nested host call.
use super::tests::{next_event, temp_paths};
use super::{PluginEvent, PluginStatus, PluginSupervisor, SupervisorOptions};
use crate::{config::ConfigWatcher, providers::ProviderHub};
use serde_json::json;
use std::{fs, sync::Arc};

#[tokio::test]
async fn command_returns_real_reply_and_a_nested_host_request_cannot_deadlock() {
    let (_temp, paths) = temp_paths();
    let dir = paths.plugins_dir().join("commands");
    fs::create_dir_all(&dir).expect("folder");
    fs::write(
        dir.join("smabar.json"),
        r#"{
        "id":"commands","name":"Commands","version":"1.0.0","protocolVersion":1,
        "runtime":"exec","command":["python3","main.py"],"tiles":[{"id":"main","name":"Main"}]
    }"#,
    )
    .expect("manifest");
    fs::write(dir.join("main.py"), r#"
import json, sys
def send(value):
    sys.stdout.write(json.dumps(value)+'\n')
    sys.stdout.flush()
pending = None
for line in sys.stdin:
    msg = json.loads(line)
    method = msg.get('method')
    if method == 'initialize':
        send({'jsonrpc':'2.0','id':msg['id'],'result':{'commands':[{'name':'save','description':'Save','inputSchema':{},'outputSchema':{}}]}})
    elif method == 'command.call':
        pending = msg['id']
        send({'jsonrpc':'2.0','id':'nested','method':'settings.get','params':{}})
    elif msg.get('id') == 'nested':
        send({'jsonrpc':'2.0','id':pending,'result':{'committed':True,'settings':msg['result']}})
    elif method in ('ping','shutdown'):
        send({'jsonrpc':'2.0','id':msg['id'],'result':{}})
        if method == 'shutdown': break
"#).expect("script");
    let config = Arc::new(ConfigWatcher::spawn(paths.clone()).expect("watcher"));
    let supervisor = PluginSupervisor::start(
        paths,
        ProviderHub::new(),
        config,
        SupervisorOptions::default(),
    )
    .await;
    let mut events = supervisor.subscribe_events();
    loop {
        if let PluginEvent::Status { status, .. } = next_event(&mut events).await {
            match status {
                PluginStatus::Running => break,
                PluginStatus::Failed => panic!("command test plugin failed to start"),
                _ => {}
            }
        }
    }
    assert_eq!(
        supervisor.commands("commands").expect("commands")[0].name,
        "save"
    );
    assert!(
        supervisor
            .call("commands", "missing", json!({}))
            .await
            .is_err()
    );
    assert!(
        supervisor
            .call("commands", "save", json!("wrong shape"))
            .await
            .is_err()
    );
    let reply = supervisor
        .call("commands", "save", json!({}))
        .await
        .expect("nested request succeeds");
    assert_eq!(reply["committed"], true);
    supervisor.shutdown_all().await;
    assert!(supervisor.commands("commands").is_err());
}

#[test]
fn malformed_or_duplicate_command_advertisements_are_rejected() {
    let valid =
        json!({"name":"todos.list","description":"List", "inputSchema":{},"outputSchema":{}});
    assert!(
        super::commands::parse_commands(&json!({}))
            .expect("old plugin")
            .is_empty()
    );
    assert!(super::commands::parse_commands(&json!({"commands":[valid.clone(),valid]})).is_err());
    assert!(super::commands::parse_commands(&json!({"commands":[{"name":"x"}]})).is_err());
}

#[tokio::test]
async fn replacing_a_session_cancels_old_callbacks_without_removing_the_new_one() {
    let sessions = Arc::new(super::commands::Sessions::default());
    let (tx, mut rx) = tokio::sync::mpsc::channel(2);
    let rpc = super::rpc::RpcClient::new(tx, super::rpc::PendingMap::default());
    let old = sessions.enter("example", rpc.clone(), vec![]);
    let owner = super::HostSession {
        plugin_id: "example".into(),
        generation: old.session.generation,
        plugin_dir: Default::default(),
        data_dir: Default::default(),
        tiles: vec!["main".into()],
        stopped: old.session.stopped.clone(),
        rpc: rpc.clone(),
    };
    let current = sessions.enter("example", rpc, vec![]);
    assert!(owner.stopped.is_cancelled());
    assert!(!owner.notify("event", json!({"action":"complete"})).await);
    assert!(rx.try_recv().is_err());
    drop(old);
    assert!(!current.session.stopped.is_cancelled());
    assert_eq!(
        sessions.active.lock().expect("sessions")["example"].generation,
        current.session.generation
    );
    drop(current);
    assert!(sessions.active.lock().expect("sessions").is_empty());
}
