//! Persistence of the main window's placement (`window-state.json`).
//!
//! The file keeps the last *normal* geometry and, separately, whether the
//! window was maximized: a maximized window never overwrites the normal
//! geometry, so restoring it from the maximized state brings back the size
//! and position it had before.
//!
//! The geometry is sampled in memory shortly after window events (no file
//! I/O; see [`SAMPLE_DELAY`]) and written once, atomically, when the window
//! closes or the application exits.
//! The window is created hidden (`visible: false` in `tauri.conf.json`) and
//! shown only after its saved placement has been applied, so it never appears
//! first at a wrong position.
//!
//! The position is kept in physical pixels of the virtual desktop (the
//! coordinates monitors report), the size in logical pixels, so that a window
//! keeps its apparent size on a monitor with a different scale factor. A saved
//! placement is checked against the monitors available at startup
//! ([`plan_restore`]): a missing or unreadable file, an invalid size or a
//! window no longer on any monitor falls back to a reasonable placement on an
//! available monitor; the window never ends up out of reach.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{LogicalSize, Manager, Monitor, PhysicalPosition, Runtime, WebviewWindow};

use crate::atomic_file;

pub const WINDOW_STATE_FILE_NAME: &str = "window-state.json";
const FORMAT_VERSION: u32 = 1;
/// Minimum inner size, in logical pixels (`minWidth`/`minHeight` of `tauri.conf.json`).
pub const MIN_WIDTH: f64 = 640.0;
pub const MIN_HEIGHT: f64 = 480.0;
/// Largest accepted size, in logical pixels; anything larger is corrupt data.
const MAX_DIMENSION: f64 = 100_000.0;
/// Pixels a window may extend past its monitor's work area and still be kept
/// where it is (invisible resize borders, windows snapped to an edge).
const EDGE_TOLERANCE: i32 = 16;
/// Delay between a move or resize event and the sampling of the window.
/// Windows reports the move and the new size of a maximization before the
/// window is known as maximized; sampling later never records the maximized
/// geometry as the normal one.
const SAMPLE_DELAY: Duration = Duration::from_millis(300);

/// Contents of `window-state.json`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowState {
    pub version: u32,
    /// The last geometry of the window while it was neither maximized nor
    /// minimized; `None` until the window has been moved or resized.
    #[serde(default)]
    pub normal: Option<Geometry>,
    #[serde(default)]
    pub maximized: bool,
}

/// Outer position (physical pixels) and inner size (logical pixels).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Geometry {
    pub x: i32,
    pub y: i32,
    pub width: f64,
    pub height: f64,
}

/// The work area of a monitor (physical pixels) and its scale factor.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MonitorArea {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub scale_factor: f64,
}

impl MonitorArea {
    fn from_monitor(monitor: &Monitor) -> Self {
        let area = monitor.work_area();
        Self {
            x: area.position.x,
            y: area.position.y,
            width: area.size.width,
            height: area.size.height,
            scale_factor: monitor.scale_factor(),
        }
    }

    fn right(&self) -> i64 {
        i64::from(self.x) + i64::from(self.width)
    }

    fn bottom(&self) -> i64 {
        i64::from(self.y) + i64::from(self.height)
    }

    /// Area of the intersection with a rectangle, in square physical pixels.
    fn overlap(&self, rect: &Rect) -> i64 {
        let width = self.right().min(rect.right) - i64::from(self.x).max(rect.x);
        let height = self.bottom().min(rect.bottom) - i64::from(self.y).max(rect.y);
        if width > 0 && height > 0 {
            width * height
        } else {
            0
        }
    }

    fn contains(&self, rect: &Rect) -> bool {
        let tolerance = i64::from(EDGE_TOLERANCE);
        rect.x >= i64::from(self.x) - tolerance
            && rect.y >= i64::from(self.y) - tolerance
            && rect.right <= self.right() + tolerance
            && rect.bottom <= self.bottom() + tolerance
    }
}

/// A rectangle in physical pixels (`i64` so that corrupt values cannot overflow).
struct Rect {
    x: i64,
    y: i64,
    right: i64,
    bottom: i64,
}

/// What to apply to the window before showing it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RestorePlan {
    /// `None` keeps the position chosen by the system (centered).
    pub position: Option<PhysicalPosition<i32>>,
    /// `None` keeps the configured default size.
    pub size: Option<LogicalSize<f64>>,
    pub maximized: bool,
}

impl RestorePlan {
    const DEFAULT: Self = Self {
        position: None,
        size: None,
        maximized: false,
    };
}

/// Decides how to place the window from the saved state and the monitors
/// available now. `primary` indexes `monitors`.
pub fn plan_restore(
    saved: Option<&WindowState>,
    monitors: &[MonitorArea],
    primary: Option<usize>,
) -> RestorePlan {
    let Some(saved) = saved else {
        return RestorePlan::DEFAULT;
    };
    let maximized = saved.maximized;
    let Some(geometry) = saved.normal.filter(is_valid) else {
        // Without a usable normal geometry the default one is restored
        // when the user leaves the maximized state.
        return RestorePlan {
            maximized,
            ..RestorePlan::DEFAULT
        };
    };
    let fallback = primary
        .and_then(|index| monitors.get(index))
        .or(monitors.first());
    let Some(fallback) = fallback else {
        // No monitor information: keep the size, let the system place it.
        return RestorePlan {
            position: None,
            size: Some(clamp_size(geometry.width, geometry.height, None)),
            maximized,
        };
    };

    let rect = window_rect(&geometry, monitor_at(monitors, geometry.x, geometry.y));
    if let Some(monitor) = monitors.iter().find(|monitor| monitor.contains(&rect)) {
        return RestorePlan {
            position: Some(PhysicalPosition::new(geometry.x, geometry.y)),
            size: Some(clamp_size(geometry.width, geometry.height, Some(monitor))),
            maximized,
        };
    }
    // Partly or entirely off screen: move it fully onto the monitor showing
    // the largest part of it, or onto the primary monitor.
    let target = monitors
        .iter()
        .filter(|monitor| monitor.overlap(&rect) > 0)
        .max_by_key(|monitor| monitor.overlap(&rect))
        .unwrap_or(fallback);
    let size = clamp_size(geometry.width, geometry.height, Some(target));
    let (width, height) = physical(size, target.scale_factor);
    let position = if monitors.iter().any(|monitor| monitor.overlap(&rect) > 0) {
        PhysicalPosition::new(
            clamp_axis(geometry.x, width, target.x, target.width),
            clamp_axis(geometry.y, height, target.y, target.height),
        )
    } else {
        centered(target, width, height)
    };
    RestorePlan {
        position: Some(position),
        size: Some(size),
        maximized,
    }
}

fn is_valid(geometry: &Geometry) -> bool {
    geometry.width.is_finite()
        && geometry.height.is_finite()
        && geometry.width > 0.0
        && geometry.height > 0.0
        && geometry.width <= MAX_DIMENSION
        && geometry.height <= MAX_DIMENSION
}

/// The monitor containing a point, used for the scale factor of a geometry.
fn monitor_at(monitors: &[MonitorArea], x: i32, y: i32) -> Option<&MonitorArea> {
    let point = Rect {
        x: i64::from(x),
        y: i64::from(y),
        right: i64::from(x) + 1,
        bottom: i64::from(y) + 1,
    };
    monitors.iter().find(|monitor| monitor.overlap(&point) > 0)
}

fn window_rect(geometry: &Geometry, monitor: Option<&MonitorArea>) -> Rect {
    let scale = monitor.map_or(1.0, |monitor| monitor.scale_factor);
    let (width, height) = physical(LogicalSize::new(geometry.width, geometry.height), scale);
    Rect {
        x: i64::from(geometry.x),
        y: i64::from(geometry.y),
        right: i64::from(geometry.x) + i64::from(width),
        bottom: i64::from(geometry.y) + i64::from(height),
    }
}

/// Keeps the size between the minimum size and the monitor's work area.
fn clamp_size(width: f64, height: f64, monitor: Option<&MonitorArea>) -> LogicalSize<f64> {
    let (max_width, max_height) = monitor.map_or((MAX_DIMENSION, MAX_DIMENSION), |monitor| {
        (
            f64::from(monitor.width) / monitor.scale_factor,
            f64::from(monitor.height) / monitor.scale_factor,
        )
    });
    // A work area smaller than the minimum size wins over the minimum.
    LogicalSize::new(
        width.max(MIN_WIDTH).min(max_width).max(1.0),
        height.max(MIN_HEIGHT).min(max_height).max(1.0),
    )
}

fn physical(size: LogicalSize<f64>, scale_factor: f64) -> (u32, u32) {
    let scale = if scale_factor.is_finite() && scale_factor > 0.0 {
        scale_factor
    } else {
        1.0
    };
    // Sizes are at most MAX_DIMENSION times a sane scale; the casts saturate.
    (
        (size.width * scale).round() as u32,
        (size.height * scale).round() as u32,
    )
}

/// Moves a span of `length` starting at `start` inside `[origin, origin + extent]`.
fn clamp_axis(start: i32, length: u32, origin: i32, extent: u32) -> i32 {
    let max_start = i64::from(origin) + i64::from(extent) - i64::from(length);
    let clamped = i64::from(start).min(max_start).max(i64::from(origin));
    i32::try_from(clamped).unwrap_or(origin)
}

fn centered(monitor: &MonitorArea, width: u32, height: u32) -> PhysicalPosition<i32> {
    let offset = |extent: u32, length: u32| {
        i32::try_from(extent.saturating_sub(length) / 2).unwrap_or_default()
    };
    PhysicalPosition::new(
        monitor.x.saturating_add(offset(monitor.width, width)),
        monitor.y.saturating_add(offset(monitor.height, height)),
    )
}

/// Reads the saved state; a missing, unreadable or invalid file is ignored.
pub fn load(path: &Path) -> Option<WindowState> {
    let content = fs::read(path).ok()?;
    let state: WindowState = serde_json::from_slice(&content).ok()?;
    (state.version == FORMAT_VERSION).then_some(state)
}

pub fn save(path: &Path, state: &WindowState) -> std::io::Result<()> {
    let content = serde_json::to_vec_pretty(state).map_err(std::io::Error::other)?;
    atomic_file::write(path, &content)
}

/// Managed state: the window placement tracked in memory and saved on exit.
pub struct WindowStateTracker {
    path: Option<PathBuf>,
    state: Mutex<WindowState>,
    /// A delayed sampling is scheduled.
    sampling: AtomicBool,
}

impl WindowStateTracker {
    pub fn new(config_dir: Option<&Path>) -> Self {
        let path = config_dir.map(|dir| dir.join(WINDOW_STATE_FILE_NAME));
        let saved = path.as_deref().and_then(load);
        Self {
            path,
            state: Mutex::new(saved.unwrap_or(WindowState {
                version: FORMAT_VERSION,
                normal: None,
                maximized: false,
            })),
            sampling: AtomicBool::new(false),
        }
    }

    /// Applies the saved placement to the hidden window, then shows it.
    /// The window is shown even when the placement cannot be applied.
    pub fn restore<R: Runtime>(&self, window: &WebviewWindow<R>) {
        let saved = self.path.as_deref().and_then(load);
        let monitors: Vec<Monitor> = window.available_monitors().unwrap_or_default();
        let primary = window.primary_monitor().ok().flatten();
        let primary_index = primary.as_ref().and_then(|primary| {
            monitors
                .iter()
                .position(|monitor| monitor.position() == primary.position())
        });
        let areas: Vec<MonitorArea> = monitors.iter().map(MonitorArea::from_monitor).collect();
        let plan = plan_restore(saved.as_ref(), &areas, primary_index);
        // Moving first lets the system apply the target monitor's scale
        // factor before the logical size is converted.
        if let Some(position) = plan.position {
            let _ = window.set_position(position);
        }
        if let Some(size) = plan.size {
            let _ = window.set_size(size);
        }
        if plan.maximized {
            let _ = window.maximize();
        }
        let _ = window.show();
        let _ = window.set_focus();
    }

    /// Samples the placement shortly after a move or resize event, once per
    /// burst of events (see [`SAMPLE_DELAY`]).
    pub fn schedule_track<R: Runtime>(&self, window: &WebviewWindow<R>) {
        if self.sampling.swap(true, Ordering::SeqCst) {
            return;
        }
        let window = window.clone();
        std::thread::spawn(move || {
            std::thread::sleep(SAMPLE_DELAY);
            let tracker = window.state::<WindowStateTracker>();
            tracker.sampling.store(false, Ordering::SeqCst);
            tracker.track(&window);
        });
    }

    /// Records the current placement (sampled after move and resize events,
    /// and before closing). A minimized window keeps the previous values; a
    /// maximized one keeps the previous normal geometry.
    pub fn track<R: Runtime>(&self, window: &WebviewWindow<R>) {
        if window.is_minimized().unwrap_or(true) {
            return;
        }
        let Ok(maximized) = window.is_maximized() else {
            return;
        };
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        state.maximized = maximized;
        if maximized {
            return;
        }
        let (Ok(position), Ok(size), Ok(scale)) = (
            window.outer_position(),
            window.inner_size(),
            window.scale_factor(),
        ) else {
            return;
        };
        let size = size.to_logical::<f64>(scale);
        let geometry = Geometry {
            x: position.x,
            y: position.y,
            width: size.width,
            height: size.height,
        };
        if is_valid(&geometry) {
            state.normal = Some(geometry);
        }
    }

    /// Writes the tracked placement. Errors only lose the placement.
    pub fn save(&self) {
        let Some(path) = &self.path else {
            return;
        };
        let state = *self.state.lock().unwrap_or_else(PoisonError::into_inner);
        let _ = save(path, &state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FULL_HD: MonitorArea = MonitorArea {
        x: 0,
        y: 0,
        width: 1920,
        height: 1040,
        scale_factor: 1.0,
    };
    /// A 4K monitor at 150 % on the right of [`FULL_HD`].
    const RIGHT_4K: MonitorArea = MonitorArea {
        x: 1920,
        y: 0,
        width: 3840,
        height: 2100,
        scale_factor: 1.5,
    };

    fn state(normal: Option<Geometry>, maximized: bool) -> WindowState {
        WindowState {
            version: FORMAT_VERSION,
            normal,
            maximized,
        }
    }

    fn geometry(x: i32, y: i32, width: f64, height: f64) -> Geometry {
        Geometry {
            x,
            y,
            width,
            height,
        }
    }

    #[test]
    fn first_start_keeps_the_configured_placement() {
        assert_eq!(
            plan_restore(None, &[FULL_HD], Some(0)),
            RestorePlan::DEFAULT
        );
    }

    #[test]
    fn restores_a_normal_window_exactly() {
        let saved = state(Some(geometry(100, 80, 1200.0, 800.0)), false);

        let plan = plan_restore(Some(&saved), &[FULL_HD], Some(0));

        assert_eq!(
            plan,
            RestorePlan {
                position: Some(PhysicalPosition::new(100, 80)),
                size: Some(LogicalSize::new(1200.0, 800.0)),
                maximized: false,
            }
        );
    }

    #[test]
    fn a_maximized_window_keeps_its_normal_geometry_for_later() {
        let saved = state(Some(geometry(100, 80, 1200.0, 800.0)), true);

        let plan = plan_restore(Some(&saved), &[FULL_HD], Some(0));

        assert!(plan.maximized);
        assert_eq!(plan.position, Some(PhysicalPosition::new(100, 80)));
        assert_eq!(plan.size, Some(LogicalSize::new(1200.0, 800.0)));
    }

    #[test]
    fn a_maximized_window_without_normal_geometry_uses_the_defaults() {
        let plan = plan_restore(Some(&state(None, true)), &[FULL_HD], Some(0));

        assert_eq!(
            plan,
            RestorePlan {
                maximized: true,
                ..RestorePlan::DEFAULT
            }
        );
    }

    #[test]
    fn restores_a_window_on_a_secondary_monitor_with_another_scale() {
        // 1200x800 logical at 150 % is 1800x1200 physical: inside the 4K area.
        let saved = state(Some(geometry(2200, 100, 1200.0, 800.0)), false);

        let plan = plan_restore(Some(&saved), &[FULL_HD, RIGHT_4K], Some(0));

        assert_eq!(plan.position, Some(PhysicalPosition::new(2200, 100)));
        assert_eq!(plan.size, Some(LogicalSize::new(1200.0, 800.0)));
    }

    #[test]
    fn a_window_of_a_disconnected_monitor_is_centered_on_the_primary_monitor() {
        let saved = state(Some(geometry(2200, 100, 1200.0, 800.0)), false);

        let plan = plan_restore(Some(&saved), &[FULL_HD], Some(0));

        assert_eq!(plan.position, Some(PhysicalPosition::new(360, 120)));
        assert_eq!(plan.size, Some(LogicalSize::new(1200.0, 800.0)));
    }

    #[test]
    fn far_off_screen_coordinates_are_brought_back() {
        let saved = state(Some(geometry(-32_000, -32_000, 800.0, 600.0)), false);

        let plan = plan_restore(Some(&saved), &[FULL_HD], Some(0));

        assert_eq!(plan.position, Some(PhysicalPosition::new(560, 220)));
    }

    #[test]
    fn a_partly_visible_window_is_moved_inside_its_monitor() {
        let saved = state(Some(geometry(1500, 700, 800.0, 600.0)), false);

        let plan = plan_restore(Some(&saved), &[FULL_HD], Some(0));

        assert_eq!(plan.position, Some(PhysicalPosition::new(1120, 440)));
    }

    #[test]
    fn edges_within_the_tolerance_are_kept() {
        // Windows snapped to the left edge report an outer x of about -7.
        let saved = state(Some(geometry(-7, 0, 960.0, 1040.0)), false);

        let plan = plan_restore(Some(&saved), &[FULL_HD], Some(0));

        assert_eq!(plan.position, Some(PhysicalPosition::new(-7, 0)));
    }

    #[test]
    fn a_window_larger_than_the_new_resolution_is_shrunk_to_fit() {
        let saved = state(Some(geometry(0, 0, 2500.0, 1400.0)), false);
        let small = MonitorArea {
            width: 1280,
            height: 680,
            ..FULL_HD
        };

        let plan = plan_restore(Some(&saved), &[small], Some(0));

        assert_eq!(plan.position, Some(PhysicalPosition::new(0, 0)));
        assert_eq!(plan.size, Some(LogicalSize::new(1280.0, 680.0)));
    }

    #[test]
    fn invalid_sizes_are_rejected_or_clamped() {
        for invalid in [0.0, -5.0, f64::NAN, f64::INFINITY, 1.0e9] {
            let saved = state(Some(geometry(10, 10, invalid, 600.0)), false);
            assert_eq!(
                plan_restore(Some(&saved), &[FULL_HD], Some(0)),
                RestorePlan::DEFAULT,
                "{invalid}"
            );
        }
        let tiny = state(Some(geometry(10, 10, 100.0, 50.0)), false);
        assert_eq!(
            plan_restore(Some(&tiny), &[FULL_HD], Some(0)).size,
            Some(LogicalSize::new(MIN_WIDTH, MIN_HEIGHT))
        );
    }

    #[test]
    fn without_monitor_information_only_the_size_is_restored() {
        let saved = state(Some(geometry(10, 10, 1200.0, 800.0)), true);

        let plan = plan_restore(Some(&saved), &[], None);

        assert_eq!(
            plan,
            RestorePlan {
                position: None,
                size: Some(LogicalSize::new(1200.0, 800.0)),
                maximized: true,
            }
        );
    }

    #[test]
    fn missing_corrupt_or_unknown_files_are_ignored() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join(WINDOW_STATE_FILE_NAME);
        assert_eq!(load(&path), None);

        for content in [
            "",
            "{",
            "[]",
            r#"{"version": 99, "maximized": true}"#,
            r#"{"version": 1, "normal": {"x": "a"}}"#,
        ] {
            fs::write(&path, content).expect("write");
            assert_eq!(load(&path), None, "{content}");
        }
    }

    #[test]
    fn saves_and_loads_the_state() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("nested").join(WINDOW_STATE_FILE_NAME);
        let saved = state(Some(geometry(-1200, 40, 1100.5, 700.0)), true);

        save(&path, &saved).expect("save");

        assert_eq!(load(&path), Some(saved));
        let json: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).expect("read")).expect("json");
        assert_eq!(
            json,
            serde_json::json!({
                "version": 1,
                "normal": { "x": -1200, "y": 40, "width": 1100.5, "height": 700.0 },
                "maximized": true
            })
        );
    }

    #[test]
    fn a_tracker_without_a_file_starts_empty() {
        let dir = tempfile::tempdir().expect("temp dir");
        let tracker = WindowStateTracker::new(Some(dir.path()));
        tracker.save();

        assert_eq!(
            load(&dir.path().join(WINDOW_STATE_FILE_NAME)),
            Some(state(None, false))
        );
    }
}
