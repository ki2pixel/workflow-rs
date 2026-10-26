use crate::services::AppServices;
use crate::single_instance::Primary;
use crate::tray::TauriTray;
use crate::Args;
use anyhow::Result;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;
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

pub fn run(args: Args, services: AppServices, primary: Option<Primary>) -> Result<()> {
    let server_url = args.server_url.clone();
    let no_tray = args.no_tray;
    let close_to_tray = !args.no_close_to_tray;
    tauri::Builder::default()
        .invoke_system(A_STRICT_IPC_STUB)
        .manage(services)
        .setup(move |app| {
            let mut primary = primary;
            let mut activation_receiver = None;
            if let Some(primary) = primary.take() {
                app.manage(primary.guard);
                activation_receiver = Some(primary.receiver);
            }

            let mut window_config = app
                .config()
                .app
                .windows
                .first()
                .expect("fenêtre main dans tauri.conf.json")
                .clone();
            window_config.url = tauri::WebviewUrl::External(server_url.clone());

            let window = tauri::WebviewWindowBuilder::from_config(app.handle(), &window_config)?
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

            // `gtk_window_present_with_time(GDK_CURRENT_TIME)` ne suffit pas
            // sous GNOME quand la fenêtre est minimisée. On résout l'identifiant
            // X11 une fois, puis on envoie `_NET_ACTIVE_WINDOW` à chaque
            // activation. La résolution passe par un thread non principal car
            // `window_handle()` attend une réponse de la boucle d'événements.
            let x11_window_id = Arc::new(AtomicU32::new(0));
            let handle_window = window.clone();
            let handle_window_id = x11_window_id.clone();
            tauri::async_runtime::spawn(async move {
                for _ in 0..50 {
                    match handle_window.window_handle() {
                        Ok(handle) => {
                            let id = match handle.as_raw() {
                                RawWindowHandle::Xlib(handle) => handle.window as u32,
                                RawWindowHandle::Xcb(handle) => handle.window.get(),
                                _ => 0,
                            };
                            if id != 0 {
                                handle_window_id.store(id, Ordering::Relaxed);
                                tracing::info!(window_id = id, "identifiant X11 résolu");
                                return;
                            }
                        }
                        Err(error) => {
                            tracing::debug!(%error, "identifiant X11 indisponible");
                        }
                    }
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
                tracing::warn!("identifiant X11 non résolu; focus GTK utilisé");
            });

            if let Some(mut receiver) = activation_receiver {
                let handle = app.handle().clone();
                let x11_window_id = x11_window_id.clone();
                tauri::async_runtime::spawn(async move {
                    while let Some(activation) = receiver.recv().await {
                        let app = handle.clone();
                        let x11_window_id = x11_window_id.clone();
                        if let Err(error) = handle.run_on_main_thread(move || {
                            if let Some(window) = app.get_webview_window("main") {
                                if let Err(error) = window.show() {
                                    tracing::warn!(%error, "restauration de la fenêtre impossible");
                                }
                                if let Err(error) = window.unminimize() {
                                    tracing::warn!(%error, "déminimisation de la fenêtre impossible");
                                }
                                let window_id = x11_window_id.load(Ordering::Relaxed);
                                if window_id == 0 {
                                    if let Err(error) = window.set_focus() {
                                        tracing::warn!(%error, "focus GTK impossible");
                                    }
                                } else {
                                    // L'activation X11 doit arriver après les
                                    // messages Show/Unminimize de Tauri, sinon
                                    // `present_with_time(0)` peut replacer la
                                    // fenêtre en demande d'attention.
                                    tauri::async_runtime::spawn(async move {
                                        tokio::time::sleep(Duration::from_millis(500)).await;
                                        match crate::x11_activation::activate(window_id) {
                                            Ok(()) => {
                                                tracing::info!(window_id, "activation X11 envoyée")
                                            }
                                            Err(error) => tracing::warn!(
                                                %error,
                                                window_id,
                                                "activation X11 impossible"
                                            ),
                                        }
                                    });
                                }
                            } else {
                                tracing::warn!("fenêtre main absente lors de l'activation");
                            }
                            tracing::info!(?activation, "activation d'une instance existante");
                        }) {
                            tracing::warn!(%error, "planification de l'activation impossible");
                        }
                    }
                });
            }

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
