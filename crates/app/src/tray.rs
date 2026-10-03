//! The notification-area icon. Double-clicking it opens the settings panel,
//! which is where everything is controlled from; there is no menu.

use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Gdi::{CreateBitmap, DeleteObject};
use windows::Win32::UI::Shell::{
    NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW, Shell_NotifyIconW,
};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::Result;

use crate::window::WM_APP_TRAY;

const ICON_SIDE: usize = 32;
const TIP: &str = "Sonic Veil: double-click for settings";

/// Three rounded bars on a violet tile, drawn into premultiplied BGRA. Made
/// in code so the exe needs no resource file.
fn icon_pixels() -> Vec<u8> {
    let side = ICON_SIDE as f32;
    let mut pixels = vec![0u8; ICON_SIDE * ICON_SIDE * 4];
    let bars = [(8.0, 14.0), (14.5, 7.0), (21.0, 17.0)];
    for (i, px) in pixels.as_chunks_mut::<4>().0.iter_mut().enumerate() {
        let (x, y) = ((i % ICON_SIDE) as f32 + 0.5, (i / ICON_SIDE) as f32 + 0.5);
        // Rounded-square tile: distance outside a square inset by the corner radius.
        let radius = 7.0;
        let dx = (x - side / 2.0).abs() - (side / 2.0 - radius);
        let dy = (y - side / 2.0).abs() - (side / 2.0 - radius);
        let outside = dx.max(0.0).hypot(dy.max(0.0)) - radius;
        let alpha = (0.5 - outside).clamp(0.0, 1.0);
        let on_bar = bars
            .iter()
            .any(|&(left, top)| x >= left && x < left + 3.5 && y >= top && y < 25.0);
        let (r, g, b) = if on_bar {
            (255.0, 255.0, 255.0)
        } else {
            (124.0, 92.0, 255.0)
        };
        *px = [
            (b * alpha) as u8,
            (g * alpha) as u8,
            (r * alpha) as u8,
            (255.0 * alpha) as u8,
        ];
    }
    pixels
}

fn create_icon() -> Result<HICON> {
    let pixels = icon_pixels();
    let mask = [0u8; ICON_SIDE * ICON_SIDE / 8];
    let side = ICON_SIDE as i32;
    unsafe {
        let color = CreateBitmap(side, side, 1, 32, Some(pixels.as_ptr().cast()));
        let mask = CreateBitmap(side, side, 1, 1, Some(mask.as_ptr().cast()));
        let info = ICONINFO {
            fIcon: true.into(),
            xHotspot: 0,
            yHotspot: 0,
            hbmMask: mask,
            hbmColor: color,
        };
        let icon = CreateIconIndirect(&info);
        // The icon keeps its own copies.
        let _ = DeleteObject(color.into());
        let _ = DeleteObject(mask.into());
        icon
    }
}

pub struct Tray {
    hwnd: HWND,
    icon: HICON,
}

impl Tray {
    pub fn new(hwnd: HWND) -> Result<Self> {
        let tray = Self {
            hwnd,
            icon: create_icon()?,
        };
        tray.add();
        Ok(tray)
    }

    fn data(&self) -> NOTIFYICONDATAW {
        let mut data = NOTIFYICONDATAW {
            cbSize: size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: self.hwnd,
            uID: 1,
            uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
            uCallbackMessage: WM_APP_TRAY,
            hIcon: self.icon,
            ..Default::default()
        };
        for (slot, unit) in data.szTip.iter_mut().zip(TIP.encode_utf16()) {
            *slot = unit;
        }
        data
    }

    /// Also called when Explorer restarts and forgets every icon.
    pub fn add(&self) {
        let _ = unsafe { Shell_NotifyIconW(NIM_ADD, &self.data()) };
    }

    pub fn remove(&self) {
        let _ = unsafe { Shell_NotifyIconW(NIM_DELETE, &self.data()) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_icon_has_a_solid_tile_transparent_corners_and_white_bars() {
        let pixels = icon_pixels();
        let at = |x: usize, y: usize| &pixels[(y * ICON_SIDE + x) * 4..][..4];
        assert_eq!(at(0, 0)[3], 0, "the corner is cut off");
        assert_eq!(at(16, 28), [255, 92, 124, 255], "the tile is violet");
        assert_eq!(at(15, 20), [255, 255, 255, 255], "the middle bar is white");
    }
}
