//! The tray icon. Left click opens kidpi; right click opens kidpi's own menu
//! window (Windows' native tray menu cannot be styled).

use tauri::image::Image;
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize};

const ID: &str = "kidpi";

fn icon(on: bool) -> Image<'static> {
    let bytes: &'static [u8] = if on {
        include_bytes!("../icons/tray-on.png")
    } else {
        include_bytes!("../icons/tray-off.png")
    };
    Image::from_bytes(bytes).expect("tray icon")
}

pub fn build(app: &AppHandle) -> tauri::Result<()> {
    TrayIconBuilder::with_id(ID)
        .icon(icon(false))
        .tooltip("kidpi — обход выключен")
        .on_tray_icon_event(|tray, event| {
            let app = tray.app_handle();
            if let TrayIconEvent::Click { button, button_state: MouseButtonState::Up, position, .. } = event {
                match button {
                    MouseButton::Left => show_main(app),
                    MouseButton::Right => show_menu(app, position),
                    _ => {}
                }
            }
        })
        .build(app)?;
    Ok(())
}

pub fn set_on(app: &AppHandle, on: bool) {
    if let Some(t) = app.tray_by_id(ID) {
        let _ = t.set_icon(Some(icon(on)));
        let _ = t.set_tooltip(Some(if on { "kidpi — обход включён" } else { "kidpi — обход выключен" }));
    }
}

pub fn show_main(app: &AppHandle) {
    hide_menu(app);
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
        let _ = w.emit("window-shown", ());
    }
}

pub fn hide_menu(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("traymenu") {
        let _ = w.hide();
    }
}

/// Opens the menu window next to the cursor, inside the monitor the tray is on.
fn show_menu(app: &AppHandle, at: PhysicalPosition<f64>) {
    let Some(w) = app.get_webview_window("traymenu") else { return };
    let size = w.outer_size().unwrap_or(PhysicalSize::new(296, 336));
    let (cw, ch) = (size.width as i32, size.height as i32);
    let (px, py) = (at.x as i32, at.y as i32);
    let mut x = px - cw + 12;
    let mut y = py - ch - 10;
    if let Ok(monitors) = w.available_monitors() {
        let m = monitors.iter().find(|m| {
            let (p, s) = (m.position(), m.size());
            px >= p.x && px < p.x + s.width as i32 && py >= p.y && py < p.y + s.height as i32
        });
        if let Some(m) = m {
            let (p, s) = (m.position(), m.size());
            let (left, right) = (p.x + 8, p.x + s.width as i32 - cw - 8);
            x = x.clamp(left, right.max(left));
            if y < p.y + 8 {
                // taskbar at the top of the screen
                y = py + 10;
            }
            y = y.min(p.y + s.height as i32 - ch - 8);
        }
    }
    let _ = w.set_position(PhysicalPosition::new(x, y));
    let _ = w.show();
    let _ = w.set_focus();
    let _ = w.emit("tray-shown", ());
}
