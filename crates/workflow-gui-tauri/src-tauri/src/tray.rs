use crate::services::AppServices;
use anyhow::Result;
use std::sync::Mutex;
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Wry};
use workflow_desktop_services::tray::{TrayIconKey, TrayPresenter, TrayStatus, TrayVisualKey};
use workflow_desktop_services::ServicesError;

const OFFLINE: &[u8] = include_bytes!("../icons/tray/tray-offline.png");
const DEGRADED: &[u8] = include_bytes!("../icons/tray/tray-degraded.png");
const IDLE: &[u8] = include_bytes!("../icons/tray/tray-idle.png");
const RUNNING: &[u8] = include_bytes!("../icons/tray/tray-running.png");
const SUCCESS: &[u8] = include_bytes!("../icons/tray/tray-success.png");
const ERROR: &[u8] = include_bytes!("../icons/tray/tray-error.png");

pub struct TauriTray {
    icon: TrayIcon<Wry>,
    status_item: MenuItem<Wry>,
    step_item: MenuItem<Wry>,
    progress_item: MenuItem<Wry>,
    last_visual: Mutex<Option<TrayVisualKey>>,
}

impl TauriTray {
    pub fn install(app: &AppHandle<Wry>) -> Result<Self> {
        let status = MenuItem::with_id(app, "status", "État : démarrage…", false, None::<&str>)?;
        let step = MenuItem::with_id(app, "step", "Aucune étape", false, None::<&str>)?;
        let progress = MenuItem::with_id(app, "progress", "", false, None::<&str>)?;
        let toggle = MenuItem::with_id(app, "toggle", "Masquer la fenêtre", true, None::<&str>)?;
        let logs = MenuItem::with_id(
            app,
            "logs",
            "Ouvrir le dossier de télémétrie",
            true,
            None::<&str>,
        )?;
        let quit = MenuItem::with_id(app, "quit", "Quitter", true, None::<&str>)?;
        let separator1 = PredefinedMenuItem::separator(app)?;
        let separator2 = PredefinedMenuItem::separator(app)?;
        let menu = Menu::with_items(
            app,
            &[
                &status,
                &step,
                &progress,
                &separator1,
                &toggle,
                &logs,
                &separator2,
                &quit,
            ],
        )?;

        let toggle_for_menu = toggle.clone();
        let toggle_for_tray = toggle.clone();
        let icon = TrayIconBuilder::with_id("main")
            .icon(Image::from_bytes(IDLE)?)
            .menu(&menu)
            .show_menu_on_left_click(true)
            .on_menu_event(move |app, event| match event.id().as_ref() {
                "toggle" => toggle_window(app, Some(&toggle_for_menu)),
                "logs" => open_telemetry(app),
                "quit" => app.exit(0),
                _ => {}
            })
            .on_tray_icon_event(move |tray, event| {
                if let TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } = event
                {
                    toggle_window(tray.app_handle(), Some(&toggle_for_tray));
                }
            })
            .build(app)?;

        Ok(Self {
            icon,
            status_item: status,
            step_item: step,
            progress_item: progress,
            last_visual: Mutex::new(None),
        })
    }
}

impl TrayPresenter for TauriTray {
    fn apply(&self, status: &TrayStatus) -> Result<(), ServicesError> {
        let key = status.visual_key();
        {
            let mut last = self
                .last_visual
                .lock()
                .map_err(|_| ServicesError::Config("verrou tray empoisonné".to_string()))?;
            if last.as_ref() == Some(&key) {
                return Ok(());
            }
            *last = Some(key);
        }

        let bytes = match status.icon_key() {
            TrayIconKey::Offline => OFFLINE,
            TrayIconKey::Degraded => DEGRADED,
            TrayIconKey::Idle => IDLE,
            TrayIconKey::Running => RUNNING,
            TrayIconKey::Success => SUCCESS,
            TrayIconKey::Error => ERROR,
        };
        let image = Image::from_bytes(bytes)
            .map_err(|error| ServicesError::Config(format!("icône tray: {error}")))?;
        self.icon
            .set_icon(Some(image))
            .map_err(|error| ServicesError::Config(format!("set_icon: {error}")))?;
        let _ = self.icon.set_title(Some(status.headline.clone()));
        self.status_item
            .set_text(&status.headline)
            .map_err(|error| ServicesError::Config(format!("menu statut: {error}")))?;
        self.step_item
            .set_text(&status.detail)
            .map_err(|error| ServicesError::Config(format!("menu étape: {error}")))?;
        let (_, _, progress) = status.menu_lines();
        self.progress_item
            .set_text(progress)
            .map_err(|error| ServicesError::Config(format!("menu progression: {error}")))?;
        Ok(())
    }
}

fn toggle_window(app: &AppHandle<Wry>, toggle: Option<&MenuItem<Wry>>) {
    if let Some(window) = app.get_webview_window("main") {
        let visible = window.is_visible().unwrap_or(false);
        if visible {
            let _ = window.hide();
        } else {
            let _ = window.show();
            let _ = window.set_focus();
        }
        if let Some(item) = toggle {
            let _ = item.set_text(if visible {
                "Afficher la fenêtre"
            } else {
                "Masquer la fenêtre"
            });
        }
    }
}

fn open_telemetry(app: &AppHandle<Wry>) {
    let dir = app.state::<AppServices>().out_dir.clone();
    let _ = std::process::Command::new("xdg-open").arg(dir).spawn();
}
