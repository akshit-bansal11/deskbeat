//! The notification-area icon. Double-clicking it opens the settings panel;
//! a right-click opens the menu in `menu.rs`.

use deskbeat_core::mark;
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Gdi::{CreateBitmap, DeleteObject};
use windows::Win32::UI::Shell::{
    NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW, Shell_NotifyIconW,
};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::Result;

use crate::window::WM_APP_TRAY;

const ICON_SIDE: usize = 32;
const TIP: &str = "Deskbeat: double-click for settings, right-click for more";

/// The mark's three colours, as in `assets/deskbeat-dark.svg`: a paper tile,
/// which holds against a dark taskbar, ink desk and rays, and the sun in the accent.
const PAPER: [f32; 3] = [245.0, 244.0, 240.0];
const INK: [f32; 3] = [19.0, 19.0, 22.0];
const EMBER: [f32; 3] = [204.0, 53.0, 16.0];

/// The Deskbeat mark, drawn into premultiplied BGRA. Made in code so the
/// tray needs no resource file.
fn icon_pixels() -> Vec<u8> {
    let mut pixels = vec![0u8; ICON_SIDE * ICON_SIDE * 4];
    for (i, px) in pixels.as_chunks_mut::<4>().0.iter_mut().enumerate() {
        let (x, y) = ((i % ICON_SIDE) as f32 + 0.5, (i / ICON_SIDE) as f32 + 0.5);
        let (alpha, ink, ember) = mark::cover(x, y, ICON_SIDE as f32);
        let channel = |i: usize| PAPER[i] * (1.0 - ink - ember) + INK[i] * ink + EMBER[i] * ember;
        let (r, g, b) = (channel(0), channel(1), channel(2));
        *px = [
            (b * alpha).round() as u8,
            (g * alpha).round() as u8,
            (r * alpha).round() as u8,
            (255.0 * alpha).round() as u8,
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
    fn the_icon_has_a_paper_tile_transparent_corners_an_ink_desk_and_an_accent_sun() {
        let pixels = icon_pixels();
        // Blue, green, red, alpha.
        let at = |x: usize, y: usize| &pixels[(y * ICON_SIDE + x) * 4..][..4];
        let (paper, ink, ember) = ([240, 244, 245, 255], [22, 19, 19, 255], [16, 53, 204, 255]);
        assert_eq!(at(0, 0)[3], 0, "the corner is cut off");
        assert_eq!(
            (at(2, 16), at(29, 16)),
            (&paper[..], &paper[..]),
            "the tile"
        );
        assert_eq!(at(16, 24), ink, "the desk");
        assert_eq!(at(16, 9), ink, "the middle ray");
        assert_eq!(at(15, 19), ember, "the sun");
        assert_eq!(at(16, 2), paper, "the sky over it");
    }
}
