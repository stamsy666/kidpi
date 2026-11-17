//! "Масштаб" in settings: the windows' web content zooms in, and the main window
//! grows by the same factor so the layout keeps the room it was made for.

use tauri::{AppHandle, LogicalSize, Manager, PhysicalPosition, WebviewWindow};

/// The choices the settings offer, 100 % and up.
pub const STEPS: &[f64] = &[1.0, 1.1, 1.25, 1.5, 1.75, 2.0];

/// Main window size at 100 % (tauri.conf.json) and its smallest size.
const MAIN: (f64, f64) = (980.0, 660.0);
const MAIN_MIN: (f64, f64) = (760.0, 560.0);
/// Tray menu window at 100 %.
const MENU: (f64, f64) = (296.0, 336.0);

/// The nearest offered step, never below 100 %.
pub fn snap(s: f64) -> f64 {
    if !s.is_finite() {
        return 1.0;
    }
    STEPS
        .iter()
        .copied()
        .min_by(|a, b| (a - s).abs().total_cmp(&(b - s).abs()))
        .unwrap_or(1.0)
}

/// Work area (logical px, then its physical rect) of the monitor the window is on.
fn work_area(w: &WebviewWindow) -> Option<((f64, f64), (i32, i32, i32, i32))> {
    let m = w.current_monitor().ok().flatten().or_else(|| w.primary_monitor().ok().flatten())?;
    let sf = m.scale_factor();
    let a = m.work_area();
    let (x, y, pw, ph) = (a.position.x, a.position.y, a.size.width as i32, a.size.height as i32);
    Some(((pw as f64 / sf, ph as f64 / sf), (x, y, pw, ph)))
}

/// The largest step at which the main window's smallest size still fits the screen.
pub fn max(app: &AppHandle) -> f64 {
    let Some(w) = app.get_webview_window("main") else { return 1.0 };
    let Some(((lw, lh), _)) = work_area(&w) else { return 1.25 };
    STEPS
        .iter()
        .rev()
        .copied()
        .find(|s| MAIN_MIN.0 * s <= lw && MAIN_MIN.1 * s <= lh)
        .unwrap_or(1.0)
}

/// Zoom of one window's web content (also called by the page when it loads,
/// since WebView2 may forget the zoom on the first navigation).
pub fn zoom(w: &WebviewWindow, s: f64) {
    let _ = w.set_zoom(s);
    if w.label() == "traymenu" {
        let _ = w.set_size(LogicalSize::new(MENU.0 * s, MENU.1 * s));
    }
}

fn min_size(w: &WebviewWindow, s: f64) -> (f64, f64) {
    let (lw, lh) = work_area(w).map(|(l, _)| l).unwrap_or((f64::MAX, f64::MAX));
    ((MAIN_MIN.0 * s).min(lw), (MAIN_MIN.1 * s).min(lh))
}

/// On launch, before the window is shown: zoom, size for the scale, centred.
pub fn init(app: &AppHandle, s: f64) {
    if let Some(w) = app.get_webview_window("traymenu") {
        zoom(&w, s);
    }
    let Some(w) = app.get_webview_window("main") else { return };
    let _ = w.set_zoom(s);
    if s == 1.0 {
        return;
    }
    let (mw, mh) = min_size(&w, s);
    let _ = w.set_min_size(Some(LogicalSize::new(mw, mh)));
    let (lw, lh) = work_area(&w).map(|(l, _)| l).unwrap_or((f64::MAX, f64::MAX));
    let _ = w.set_size(LogicalSize::new((MAIN.0 * s).clamp(mw, lw), (MAIN.1 * s).clamp(mh, lh)));
    let _ = w.center();
}

/// A new scale from the settings: zoom both windows, and grow or shrink the main
/// window around its centre by new/old, inside the screen.
pub fn change(app: &AppHandle, old: f64, new: f64) {
    if let Some(w) = app.get_webview_window("traymenu") {
        zoom(&w, new);
    }
    let Some(w) = app.get_webview_window("main") else { return };
    let _ = w.set_zoom(new);
    let (mw, mh) = min_size(&w, new);
    let _ = w.set_min_size(Some(LogicalSize::new(mw, mh)));
    if w.is_maximized().unwrap_or(false) || w.is_fullscreen().unwrap_or(false) {
        return;
    }
    let Some(((lw, lh), (ax, ay, aw, ah))) = work_area(&w) else { return };
    let sf = w.scale_factor().unwrap_or(1.0);
    let Ok(inner) = w.inner_size() else { return };
    let k = new / old;
    let tw = (inner.width as f64 / sf * k).clamp(mw, lw);
    let th = (inner.height as f64 / sf * k).clamp(mh, lh);

    // centre before the change, in physical pixels
    let (cx, cy) = match (w.outer_position(), w.outer_size()) {
        (Ok(p), Ok(o)) => (p.x + o.width as i32 / 2, p.y + o.height as i32 / 2),
        _ => (ax + aw / 2, ay + ah / 2),
    };
    let _ = w.set_size(LogicalSize::new(tw, th));
    let (pw, ph) = ((tw * sf).round() as i32, (th * sf).round() as i32);
    let x = (cx - pw / 2).clamp(ax, (ax + aw - pw).max(ax));
    let y = (cy - ph / 2).clamp(ay, (ay + ah - ph).max(ay));
    let _ = w.set_position(PhysicalPosition::new(x, y));
}
