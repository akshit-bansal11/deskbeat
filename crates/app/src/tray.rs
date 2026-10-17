//! The notification-area icon. Double-clicking it opens the settings panel;
//! a right-click opens the menu in `menu.rs`.

use inlay_core::mark;
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Gdi::{CreateBitmap, DeleteObject};
use windows::Win32::UI::Shell::{
    NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW, Shell_NotifyIconW,
};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::Result;

use crate::window::WM_APP_TRAY;

const ICON_SIDE: usize = 32;
const TIP: &str = "Inlay: double-click for settings, right-click for more";

/// The Inlay mark, drawn into premultiplied BGRA. Made in code so the tray
/// needs no resource file.
fn icon_pixels() -> Vec<u8> {
    let mut pixels = vec![0u8; ICON_SIDE * ICON_SIDE * 4];
    for (i, px) in pixels.as_chunks_mut::<4>().0.iter_mut().enumerate() {
        let (x, y) = ((i % ICON_SIDE) as f32 + 0.5, (i / ICON_SIDE) as f32 + 0.5);
        let [r, g, b, a] = mark::pixel(x, y, ICON_SIDE as f32);
        let channel = |v: f32| (v * a * 255.0).round() as u8;
        *px = [
            channel(b),
            channel(g),
            channel(r),
            (a * 255.0).round() as u8,
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
    fn the_icon_is_the_mark_on_an_ink_tile_with_its_corners_cut() {
        let pixels = icon_pixels();
        // Blue, green, red, alpha.
        let at = |x: usize, y: usize| &pixels[(y * ICON_SIDE + x) * 4..][..4];
        let bgra =
            |c: inlay_core::color::Rgba| [c[2], c[1], c[0], 1.0].map(|v| (v * 255.0).round() as u8);
        assert_eq!(at(0, 0)[3], 0, "the corner is cut off");
        assert_eq!(at(16, 16), bgra(mark::INK), "the gap between the pieces");
        assert_eq!(at(9, 11), bgra(mark::COBALT));
        assert_eq!(at(21, 10), bgra(mark::ORANGE));
        assert_eq!(at(10, 21), bgra(mark::SUN));
        assert_eq!(at(23, 23), bgra(mark::BONE));
    }
}
