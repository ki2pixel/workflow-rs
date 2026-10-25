use crate::services::AppServices;
use crate::Args;
use anyhow::Result;
use tauri::Manager;

/// Remplace `ipc-protocol.js` : le transport IPC Tauri n'est jamais utilisable.
const A_STRICT_IPC_STUB: &str = r#"
;(function () {
  function deny() {
    throw new Error("IPC Tauri désactivé (A-Strict)");
  }
  Object.defineProperty(window.__TAURI_INTERNALS__, "postMessage", { value: deny });
  try {
    if (window.ipc && typeof window.ipc.postMessage === "function") {
      window.ipc.postMessage = deny;
    }
  } catch (_) {}
})();
"#;

/// `withGlobalTauri` est déjà false ; ce script supprime toute trace résiduelle
/// et neutralise le handler IPC natif de wry (`window.webkit.messageHandlers`).
const A_STRICT_SCRUB: &str = r#"
;(function () {
  function deny() {
    throw new Error("IPC natif désactivé (A-Strict)");
  }
  try { delete window.__TAURI__; } catch (_) {}
  try {
    var handlers = window.webkit && window.webkit.messageHandlers;
    var handler = handlers && handlers.ipc;
    if (handler && typeof handler.postMessage === "function") {
      Object.defineProperty(handler, "postMessage", {
        value: deny,
        configurable: false,
        writable: false
      });
    }
  } catch (_) {}
})();
"#;

pub fn run(args: Args, services: AppServices) -> Result<()> {
    tauri::Builder::default()
        .invoke_system(A_STRICT_IPC_STUB)
        .manage(services)
        .setup(move |app| {
            let mut window_config = app
                .config()
                .app
                .windows
                .first()
                .expect("fenêtre main dans tauri.conf.json")
                .clone();
            window_config.url = tauri::WebviewUrl::External(args.server_url.clone());

            let _window = tauri::WebviewWindowBuilder::from_config(app.handle(), &window_config)?
                .initialization_script(A_STRICT_SCRUB)
                .on_page_load(|window, payload| {
                    let services = window.state::<AppServices>();
                    let _ = std::fs::write(
                        services.out_dir.join("page-loaded.txt"),
                        format!("{}\n", payload.url()),
                    );
                    services.start_audit(window.clone());
                })
                .build()?;
            Ok(())
        })
        .build(tauri::generate_context!())?
        .run(|_app, _event| {});
    Ok(())
}
