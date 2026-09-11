//! Keeps Tauri IPC usable only from the shell's top-level document.
//!
//! Wry 0.55 injects initialization scripts into WebView2 subframes even when
//! Tauri marks them main-frame-only. Remote media players therefore receive
//! Tauri's globals on Windows. This invoke transport leaves those globals
//! inert in every subframe and never exposes the lexical integrity key.

use tauri::ipc::InvokeResponseBody;

const INVOKE_SYSTEM: &str = r#"
;(function () {
  const invokeKey = __INVOKE_KEY__
  const serializeToIpc = '__TAURI_TO_IPC_KEY__'

  Object.defineProperty(window.__TAURI_INTERNALS__, 'postMessage', {
    value: function (message) {
      if (window !== window.top) return

      const data = JSON.stringify({
        ...message,
        options: { ...(message.options || {}), customProtocolIpcBlocked: true },
        __TAURI_INVOKE_KEY__: invokeKey
      }, (_key, value) => {
        if (value instanceof Map) return Object.fromEntries(value.entries())
        if (value instanceof Uint8Array) return Array.from(value)
        if (value instanceof ArrayBuffer) return Array.from(new Uint8Array(value))
        if (typeof value === 'object' && value !== null && serializeToIpc in value) {
          return value[serializeToIpc]()
        }
        return value
      })

      window.ipc.postMessage(data)
    }
  })
})()
"#;

/// Replaces Tauri's custom-protocol invoke path and its global large-payload
/// queue. Both would otherwise be reachable from WebView2 player subframes.
pub fn isolate_subframes<R: tauri::Runtime>(builder: tauri::Builder<R>) -> tauri::Builder<R> {
    builder
        .invoke_system(INVOKE_SYSTEM)
        .channel_interceptor(|webview, callback, index, body| {
            let script = channel_callback_script(callback.0, index, body);
            if let Err(error) = webview.eval(script) {
                tracing::error!(
                    %error,
                    "Tauri channel message was dropped; retry the action and inspect this WebView error"
                );
            }
            true
        })
}

fn channel_callback_script(callback: u32, index: usize, body: &InvokeResponseBody) -> String {
    let message = match body {
        InvokeResponseBody::Json(json) => json.clone(),
        InvokeResponseBody::Raw(bytes) => {
            let bytes = bytes
                .iter()
                .map(u8::to_string)
                .collect::<Vec<_>>()
                .join(",");
            format!("new Uint8Array([{bytes}]).buffer")
        }
    };
    format!(
        "window.__TAURI_INTERNALS__.runCallback({callback}, {{message:{message},index:{index}}})"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invoke_key_is_lexical_and_subframes_stop_before_message_processing() {
        assert_eq!(INVOKE_SYSTEM.matches("__INVOKE_KEY__").count(), 1);
        assert!(INVOKE_SYSTEM.contains("if (window !== window.top) return"));
        assert!(INVOKE_SYSTEM.contains("customProtocolIpcBlocked: true"));
        assert!(INVOKE_SYSTEM.contains("window.ipc.postMessage(data)"));
        assert!(!INVOKE_SYSTEM.contains("fetch("));
        assert!(!INVOKE_SYSTEM.contains("convertFileSrc"));
    }

    #[test]
    fn channel_messages_never_use_the_global_fetch_queue() {
        let json = channel_callback_script(
            7,
            2,
            &InvokeResponseBody::Json(r#"{"ready":true}"#.to_owned()),
        );
        assert_eq!(
            json,
            r#"window.__TAURI_INTERNALS__.runCallback(7, {message:{"ready":true},index:2})"#
        );

        let raw = channel_callback_script(8, 3, &InvokeResponseBody::Raw(vec![0, 255]));
        assert_eq!(
            raw,
            "window.__TAURI_INTERNALS__.runCallback(8, {message:new Uint8Array([0,255]).buffer,index:3})"
        );
        assert!(!json.contains("fetch"));
        assert!(!raw.contains("fetch"));
    }
}
