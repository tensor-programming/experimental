use crate::edge::{self, Control, Process};

use winapi::shared::windef::HWND;

use winit::dpi::{LogicalPosition, LogicalSize};
use winit::platform::windows::WindowExtWindows;
use winit::window::Window;

pub enum HwndType {
    FillWindow,
    ConsumeHwnd(HWND),
    NewHwndInWindow,
}

pub fn new_control<F>(
    process: &Process,
    window: &Window,
    hwnd_type: HwndType,
    position: Option<LogicalPosition>,
    size: Option<LogicalSize>,
    callback: Option<F>,
) -> Result<Control, String>
where
    F: FnOnce(Control) + 'static,
{
    let window_hwnd = window.hwnd() as *mut _;
    let hwnd_type = match hwnd_type {
        HwndType::FillWindow => edge::HwndType::FillWindow(window_hwnd),
        HwndType::ConsumeHwnd(hwnd) => edge::HwndType::ConsumeHwnd(hwnd),
        HwndType::NewHwndInWindow => edge::HwndType::NewHwndInWindow(window_hwnd),
    };
    let dpi_factor = window.hidpi_factor();
    let position = position
        .unwrap_or(LogicalPosition { x: 0.0, y: 0.0 })
        .to_physical(dpi_factor)
        .into();
    let size: (u32, u32) = size
        .or(Some(window.inner_size()))
        .unwrap_or(LogicalSize {
            width: 1024.0,
            height: 768.0,
        })
        .to_physical(dpi_factor)
        .into();
    process
        .create_control(
            hwnd_type,
            position,
            (size.0 as i32, size.1 as i32),
            callback,
        )
        .map_err(|err| err.to_string())
}
