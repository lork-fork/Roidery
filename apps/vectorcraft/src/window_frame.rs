//! Keeps the custom title bar's undecorated window truly borderless on Windows (#1758).
//!
//! winit 0.30 gives an undecorated window the `WS_CAPTION | WS_BORDER` styles and hides the
//! non-client area by spreading the client rect over it (`WM_NCCALCSIZE`). On some systems the
//! desktop compositor still reserves that frame for the window surface: the content is drawn
//! offset by the caption and border while the pointer keeps the un-offset client coordinates, so
//! it no longer points at what is drawn under it (rust-windowing/winit#2419; PhotoCraft fixed the
//! same in storytold/photocraft#2401). [`service`] strips the two styles and has Windows
//! recalculate the frame; winit puts them back whenever it changes the window flags (maximize,
//! restore, …), so the returned guard runs every frame.

/// Run once per frame; a no-op once the window is borderless.
pub type WindowFrameFn = Box<dyn FnMut()>;

pub use platform::service;

#[cfg(all(windows, feature = "wgpu"))]
mod platform {
    use eframe::wgpu::rwh::{HasWindowHandle, RawWindowHandle};
    use winsafe::{HWND, HwndPlace, POINT, SIZE, co};

    /// A guard that keeps the window borderless each frame, or `None` with the system title bar
    /// (the window is decorated) or when its handle can't be found.
    pub fn service(cc: &eframe::CreationContext<'_>, custom_titlebar: bool) -> Option<super::WindowFrameFn> {
        if !custom_titlebar {
            return None;
        }
        let target = match cc.window_handle().ok()?.as_raw() {
            RawWindowHandle::Win32(h) => h.hwnd.get(),
            _ => return None,
        };
        let hwnd = find_window(target)?;
        strip(&hwnd);
        Some(Box::new(move || strip(&hwnd)))
    }

    /// The top-level window with the given raw handle: `EnumWindows` hands out real `HWND`s, so
    /// this needs no `unsafe` to wrap a raw one.
    fn find_window(target: isize) -> Option<HWND> {
        let mut found = None;
        let _ = winsafe::EnumWindows(|hwnd| {
            if hwnd.ptr() as isize == target {
                found = Some(hwnd);
                false
            } else {
                true
            }
        });
        found
    }

    /// Removes `WS_CAPTION` (which includes `WS_BORDER`) and has Windows recalculate the frame;
    /// the sizing border stays, so edge resizing and snapping keep working.
    fn strip(hwnd: &HWND) {
        let style = hwnd.style();
        let without = style & !(co::WS::CAPTION | co::WS::BORDER);
        if without.raw() == style.raw() {
            return;
        }
        hwnd.set_style(without);
        let _ = hwnd.SetWindowPos(
            HwndPlace::None,
            POINT::new(),
            SIZE::new(),
            co::SWP::FRAMECHANGED | co::SWP::NOMOVE | co::SWP::NOSIZE | co::SWP::NOZORDER | co::SWP::NOACTIVATE,
        );
    }
}

#[cfg(not(all(windows, feature = "wgpu")))]
mod platform {
    pub fn service(_cc: &eframe::CreationContext<'_>, _custom_titlebar: bool) -> Option<super::WindowFrameFn> {
        None
    }
}
