//! Screen geometry and cursor position for absolute (edge-to-edge) movement.
//!
//! Everything here is one-shot and cheap: the jiggler queries bounds and the
//! cursor origin when an edge-to-edge run starts, then drives per-tick movement
//! through the cursor backend. Coordinates use the same space the platform's
//! absolute cursor call expects:
//!
//! - Windows: virtual-desktop coordinates (`GetSystemMetrics` virtual metrics,
//!   `GetCursorPos` / `SetCursorPos`), which span every monitor including
//!   negative origins.
//! - macOS: CoreGraphics global coordinates (top-left origin), matching
//!   `CGWarpMouseCursorPosition` — deliberately *not* enigo's flipped
//!   `NSEvent::mouseLocation()` value.
//! - Linux/X11: root-window coordinates (`x11rb` `GetGeometry` /
//!   `QueryPointer`), matching enigo's X11 absolute moves. The root window
//!   already spans all monitors in a multi-screen setup.
//! - Linux/Wayland: unsupported — the backend is `ydotool` (relative only), so
//!   the jiggler falls back to the default behaviour. Availability is decided
//!   by the cursor backend's capability, not by `XDG_SESSION_TYPE`.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Bounds {
    pub min_x: i32,
    pub min_y: i32,
    pub max_x: i32,
    pub max_y: i32,
}

impl Bounds {
    pub fn clamp_x(self, x: i32) -> i32 {
        x.clamp(self.min_x, self.max_x)
    }

    pub fn clamp_y(self, y: i32) -> i32 {
        y.clamp(self.min_y, self.max_y)
    }
}

/// Combined bounds of every attached screen, or `None` where absolute
/// positioning is unavailable (e.g. Wayland/ydotool).
pub fn virtual_screen_bounds() -> Option<Bounds> {
    platform::virtual_screen_bounds()
}

/// Current cursor position in the same coordinate space as
/// [`virtual_screen_bounds`], used to capture and later restore the origin.
pub fn cursor_position() -> Option<(i32, i32)> {
    platform::cursor_position()
}

#[cfg(target_os = "windows")]
mod platform {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::UI::WindowsAndMessaging::{
        GetCursorPos, GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN,
        SM_YVIRTUALSCREEN,
    };

    use super::Bounds;

    pub fn virtual_screen_bounds() -> Option<Bounds> {
        let x = unsafe { GetSystemMetrics(SM_XVIRTUALSCREEN) };
        let y = unsafe { GetSystemMetrics(SM_YVIRTUALSCREEN) };
        let width = unsafe { GetSystemMetrics(SM_CXVIRTUALSCREEN) };
        let height = unsafe { GetSystemMetrics(SM_CYVIRTUALSCREEN) };
        if width <= 0 || height <= 0 {
            return None;
        }
        Some(Bounds {
            min_x: x,
            min_y: y,
            max_x: x + width - 1,
            max_y: y + height - 1,
        })
    }

    pub fn cursor_position() -> Option<(i32, i32)> {
        let mut point = POINT::default();
        // GetCursorPos returns windows::core::Result<()> in windows-rs 0.62.
        if unsafe { GetCursorPos(&mut point) }.is_err() {
            return None;
        }
        Some((point.x, point.y))
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use core_graphics::display::CGDisplay;
    use core_graphics::event::CGEvent;
    use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
    use core_graphics::geometry::CGRect;

    use super::Bounds;

    fn edges(rect: CGRect) -> (i32, i32, i32, i32) {
        let min_x = rect.origin.x.round() as i32;
        let min_y = rect.origin.y.round() as i32;
        let max_x = (rect.origin.x + rect.size.width).round() as i32 - 1;
        let max_y = (rect.origin.y + rect.size.height).round() as i32 - 1;
        (min_x, min_y, max_x, max_y)
    }

    pub fn virtual_screen_bounds() -> Option<Bounds> {
        let ids = CGDisplay::active_displays().ok()?;
        let mut displays = ids.into_iter().map(CGDisplay::new);
        let first = displays.next()?.bounds();
        let (mut min_x, mut min_y, mut max_x, mut max_y) = edges(first);
        for display in displays {
            let (x0, y0, x1, y1) = edges(display.bounds());
            min_x = min_x.min(x0);
            min_y = min_y.min(y0);
            max_x = max_x.max(x1);
            max_y = max_y.max(y1);
        }
        if max_x < min_x || max_y < min_y {
            return None;
        }
        Some(Bounds {
            min_x,
            min_y,
            max_x,
            max_y,
        })
    }

    pub fn cursor_position() -> Option<(i32, i32)> {
        let source = CGEventSource::new(CGEventSourceStateID::CombinedSessionState).ok()?;
        let event = CGEvent::new(source).ok()?;
        let point = event.location();
        Some((point.x.round() as i32, point.y.round() as i32))
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::ConnectionExt;

    use super::Bounds;

    pub fn virtual_screen_bounds() -> Option<Bounds> {
        let (conn, _) = x11rb::connect(None).ok()?;
        let screen = conn.setup().roots.first()?;
        let width = i32::from(screen.width_in_pixels);
        let height = i32::from(screen.height_in_pixels);
        if width <= 0 || height <= 0 {
            return None;
        }
        Some(Bounds {
            min_x: 0,
            min_y: 0,
            max_x: width - 1,
            max_y: height - 1,
        })
    }

    pub fn cursor_position() -> Option<(i32, i32)> {
        let (conn, _) = x11rb::connect(None).ok()?;
        let root = conn.setup().roots.first()?.root;
        let reply = conn.query_pointer(root).ok()?.reply().ok()?;
        Some((i32::from(reply.root_x), i32::from(reply.root_y)))
    }
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
mod platform {
    use super::Bounds;

    pub fn virtual_screen_bounds() -> Option<Bounds> {
        None
    }

    pub fn cursor_position() -> Option<(i32, i32)> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamp_stays_inside_bounds() {
        let bounds = Bounds {
            min_x: 0,
            min_y: 5,
            max_x: 100,
            max_y: 50,
        };
        assert_eq!(bounds.clamp_x(-50), 0);
        assert_eq!(bounds.clamp_x(200), 100);
        assert_eq!(bounds.clamp_x(42), 42);
        assert_eq!(bounds.clamp_y(0), 5);
        assert_eq!(bounds.clamp_y(999), 50);
    }
}
