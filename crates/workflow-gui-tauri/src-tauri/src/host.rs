use crate::services::AppServices;
use crate::tray::TauriTray;
use crate::Args;
use anyhow::Result;
use std::sync::Arc;
use tauri::Manager;
use workflow_desktop_services::tray::{NoopTray, TrayPresenter};

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
    let server_url = args.server_url.clone();
    let no_tray = args.no_tray;
    let close_to_tray = !args.no_close_to_tray;
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
            window_config.url = tauri::WebviewUrl::External(server_url.clone());

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

            let tray: Arc<dyn TrayPresenter> = if no_tray {
                Arc::new(NoopTray)
            } else {
                Arc::new(TauriTray::install(app.handle())?)
            };
            let services = app.state::<AppServices>();
            services.start_background(tray);
            Ok(())
        })
        .on_window_event(move |window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" && close_to_tray {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .build(tauri::generate_context!())?
        .run(|_app, _event| {});
    Ok(())
}
