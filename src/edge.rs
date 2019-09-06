use std::cell::RefCell;
use std::fs::File;
use std::io;
use std::io::prelude::*;
use std::mem;
use std::ptr;
use std::rc::Rc;

use winapi::shared::minwindef::{HINSTANCE, UINT};
use winapi::shared::windef::{HWND, RECT};
use winapi::shared::winerror::{S_FALSE, S_OK};
use winapi::um::winnt::LPCWSTR;
use winapi::um::{libloaderapi, winuser};
use winapi::winrt::roapi::{RoInitialize, RO_INIT_SINGLETHREADED};

use winrt::windows::foundation::{
    metadata::ApiInformation, AsyncOperationCompletedHandler, EventRegistrationToken, Rect,
    TypedEventHandler, Uri,
};
use winrt::windows::web::ui::{
    interop::{IWebViewControlSite, WebViewControl, WebViewControlProcess},
    IWebViewControl, WebViewControlScriptNotifyEventArgs,
};
use winrt::{ComPtr, FastHString, RtDefaultConstructible};

use crate::error::Error;

struct FakeSend<T>(T);
unsafe impl<T> Send for FakeSend<T> {}

struct HInstanceWrapper(HINSTANCE);
unsafe impl Sync for HInstanceWrapper {}
lazy_static! {
    static ref OUR_HINSTANCE: HInstanceWrapper =
        HInstanceWrapper(unsafe { libloaderapi::GetModuleHandleW(ptr::null()) });
}

static HOST_CLASS_NAME: [u16; 20] = [
    b'W' as u16,
    b'e' as u16,
    b'b' as u16,
    b'V' as u16,
    b'i' as u16,
    b'e' as u16,
    b'w' as u16,
    b'C' as u16,
    b'o' as u16,
    b'n' as u16,
    b't' as u16,
    b'r' as u16,
    b'o' as u16,
    b'l' as u16,
    b' ' as u16,
    b'H' as u16,
    b'o' as u16,
    b's' as u16,
    b't' as u16,
    0,
];

pub fn is_available() -> bool {
    ApiInformation::is_type_present(&FastHString::from("Windows.Web.UI.Interop.WebViewControl"))
        .unwrap_or(false)
}

unsafe fn register_host_class() {
    winuser::RegisterClassExW(&winuser::WNDCLASSEXW {
        cbSize: mem::size_of::<winuser::WNDCLASSEXW>() as UINT,
        style: winuser::CS_HREDRAW | winuser::CS_VREDRAW | winuser::CS_OWNDC,
        lpfnWndProc: Some(winuser::DefWindowProcW),
        cbClsExtra: 0,
        cbWndExtra: 0,
        hInstance: OUR_HINSTANCE.0,
        hIcon: ptr::null_mut(),
        hCursor: ptr::null_mut(),
        hbrBackground: ptr::null_mut(),
        lpszMenuName: ptr::null(),
        lpszClassName: HOST_CLASS_NAME.as_ptr(),
        hIconSm: ptr::null_mut(),
    });
}

fn new_hwnd(parent: HWND, position: (i32, i32), size: (i32, i32)) -> Result<HWND, Error> {
    unsafe {
        register_host_class();
    }

    let handle = unsafe {
        winuser::CreateWindowExW(
            0,
            HOST_CLASS_NAME.as_ptr(),
            [0].as_ptr() as LPCWSTR,
            winuser::WS_CHILD | winuser::WS_VISIBLE,
            position.0,
            position.1,
            size.0,
            size.1,
            parent,
            ptr::null_mut(),
            OUR_HINSTANCE.0,
            ptr::null_mut(),
        )
    };

    if handle.is_null() {
        return Err(Error::Io(io::Error::last_os_error()));
    }

    Ok(handle)
}

pub fn runtime_context() {
    let hr = unsafe { RoInitialize(RO_INIT_SINGLETHREADED) };
    assert!(
        hr == S_OK || hr == S_FALSE,
        "failed to call RoInitialize: error {}",
        hr
    );
}

pub enum HwndType {
    FillWindow(HWND),

    ConsumeHwnd(HWND),

    NewHwndInWindow(HWND),
}

#[derive(Clone)]
pub struct Process {
    process: ComPtr<WebViewControlProcess>,
}

impl Process {
    pub fn new() -> Process {
        let process = WebViewControlProcess::new();
        process
            .add_process_exited(&TypedEventHandler::new(move |_proc, _result| {
                eprintln!("WebViewControlProcess exited, should we do anything about it?");
                Ok(())
            }))
            .unwrap();

        Process { process }
    }

    pub fn create_control(
        &self,
        hwnd_type: HwndType,
        position: (i32, i32),
        size: (i32, i32),
        callback: Option<impl FnOnce(Control) + 'static>,
    ) -> Result<Control, Error> {
        let hwnd = match hwnd_type {
            HwndType::FillWindow(hwnd) => hwnd,
            HwndType::ConsumeHwnd(hwnd) => hwnd,
            HwndType::NewHwndInWindow(parent) => new_hwnd(parent, position, size)?,
        };

        let operation = self.process.create_web_view_control_async(
            hwnd as usize as i64,
            Rect {
                X: position.0 as f32,
                Y: position.1 as f32,
                Width: size.0 as f32,
                Height: size.1 as f32,
            },
        )?;

        let control = Control {
            inner: Rc::new(RefCell::new(ControlInner {
                hwnd,
                is_window_hwnd: match hwnd_type {
                    HwndType::FillWindow(_) => true,
                    _ => false,
                },
                control: None,
                queued_bounds_update: None,
            })),
        };

        let mut control2 = FakeSend(control.clone());
        let mut callback = FakeSend(callback);
        operation
            .set_completed(&AsyncOperationCompletedHandler::new(
                move |sender, _args| {
                    let web_view_control = unsafe { &mut *sender }.get_results().unwrap();
                    control2.0.control_created(web_view_control);
                    if let Some(callback) = callback.0.take() {
                        callback(control2.0.clone());
                    }
                    Ok(())
                },
            ))
            .unwrap();

        Ok(control)
    }
}

#[derive(Clone)]
pub struct Control {
    inner: Rc<RefCell<ControlInner>>,
}

pub struct ControlInner {
    hwnd: HWND,
    is_window_hwnd: bool,

    control: Option<ComPtr<WebViewControl>>,

    queued_bounds_update: Option<Rect>,
}

impl ControlInner {
    fn update_bounds(&mut self) -> Result<(), Error> {
        let mut rect = RECT {
            top: 0,
            left: 0,
            bottom: 0,
            right: 0,
        };
        if unsafe { winuser::GetWindowRect(self.hwnd, &mut rect) } == 0 {
            return Err(Error::Io(io::Error::last_os_error()));
        }
        self.update_bounds_from_rect(Rect {
            X: if self.is_window_hwnd {
                0.0
            } else {
                rect.left as f32
            },
            Y: if self.is_window_hwnd {
                0.0
            } else {
                rect.top as f32
            },
            Width: (rect.right - rect.left) as f32,
            Height: (rect.bottom - rect.top) as f32,
        })
    }

    fn update_bounds_from_rect(&mut self, rect: Rect) -> Result<(), Error> {
        println!("Updating bounds to {:?}", rect);
        if let Some(ref control) = self.control {
            let control_site = control.query_interface::<IWebViewControlSite>().unwrap();
            control_site.set_bounds(rect)?;
        } else {
            self.queued_bounds_update = Some(rect);
        }
        Ok(())
    }
}

impl Control {
    fn control_created(&mut self, web_view_control: Option<ComPtr<WebViewControl>>) {
        let mut inner = self.inner.borrow_mut();
        inner.control = web_view_control;
        if let Some(rect) = inner.queued_bounds_update {
            inner.queued_bounds_update = None;
            let _ = inner.update_bounds_from_rect(rect);
        }
    }

    pub fn resize(
        &self,
        position: Option<(i32, i32)>,
        size: Option<(i32, i32)>,
    ) -> Result<(), Error> {
        let mut inner = self.inner.borrow_mut();
        if !inner.is_window_hwnd {
            let (x, y) = position.unwrap_or((0, 0));
            let (width, height) = size.unwrap_or((0, 0));
            let mut flags = winuser::SWP_NOZORDER;
            if position.is_none() {
                flags |= winuser::SWP_NOMOVE;
            }
            if size.is_none() {
                flags |= winuser::SWP_NOSIZE;
            }
            unsafe {
                winuser::SetWindowPos(inner.hwnd, ptr::null_mut(), x, y, width, height, flags);
                winuser::UpdateWindow(inner.hwnd);
            }
        }
        if let Some((width, height)) = size {
            inner.update_bounds_from_rect(Rect {
                X: 0.0,
                Y: 0.0,
                Width: width as f32,
                Height: height as f32,
            })?;
        } else {
            inner.update_bounds()?;
        }
        Ok(())
    }

    pub fn get_hwnd(&self) -> HWND {
        self.inner.borrow().hwnd
    }

    pub fn get_inner(&self) -> Option<ComPtr<WebViewControl>> {
        self.inner.borrow().control.clone()
    }
}

pub trait WebView {
    type Error;
    fn navigate(&self, url: &str) -> Result<(), Self::Error>;
    fn navigate_to_string(&self, url: &str) -> Result<(), Self::Error>;
}

impl WebView for Control {
    type Error = winrt::Error;
    fn navigate(&self, url: &str) -> Result<(), winrt::Error> {
        if let Some(ref control) = self.inner.borrow().control {
            control.navigate(&*Uri::create_uri(&FastHString::from(&*url))?)?;
        }
        Ok(())
    }

    fn navigate_to_string(&self, url: &str) -> Result<(), winrt::Error> {
        let mut file = File::open(url).unwrap();
        let mut contents = String::new();
        file.read_to_string(&mut contents).unwrap();
        if let Some(ref control) = self.inner.borrow().control {
            control.navigate_to_string(&FastHString::from(contents.as_str()))?;
        }
        Ok(())
    }
}

pub struct EdgeWebViewControl {
    control: ComPtr<WebViewControl>,
}

impl EdgeWebViewControl {
    pub fn can_go_back(&self) -> bool {
        self.control.get_can_go_back().unwrap_or(false)
    }

    pub fn can_go_forward(&self) -> bool {
        self.control.get_can_go_forward().unwrap_or(false)
    }

    pub fn contains_full_screen_element(&self) -> bool {
        self.control
            .get_contains_full_screen_element()
            .unwrap_or(false)
    }

    pub fn document_title(&self) -> String {
        self.control
            .get_document_title()
            .map(|s| s.to_string())
            .unwrap_or(String::new())
    }

    pub fn capture_selected_content_to_data_package_async(&self) {}
    pub fn close(&self) {}
    pub fn get_deferred_permission_request_by_id(&self) {}
    pub fn go_back(&self) {}
    pub fn go_forward(&self) {}
    pub fn invoke_script_async(&self) {}
    pub fn move_focus(&self) {}
    pub fn navigate(&self) {}
    pub fn navigate_to_local_stream_uri(&self) {}
    pub fn navigate_to_string(&self) {}
    pub fn navigate_with_http_request_message(&self) {}
    pub fn refresh(&self) {}
    pub fn stop(&self) {}

    pub fn add_contains_full_screen_element_changed<F>(
        &self,
        f: F,
    ) -> Result<EventRegistrationToken, winrt::Error>
    where
        F: FnMut(bool) + 'static,
    {
        let mut f = FakeSend(f);
        self.control
            .add_contains_full_screen_element_changed(&TypedEventHandler::new(
                move |sender: *mut IWebViewControl, _args| {
                    let sender = unsafe { &mut *sender };
                    f.0(sender.get_contains_full_screen_element()?);
                    Ok(())
                },
            ))
    }

    pub fn add_script_notify<F>(&self, f: F) -> Result<EventRegistrationToken, winrt::Error>
    where
        F: FnMut(String) + 'static,
    {
        let mut f = FakeSend(f);
        self.control.add_script_notify(&TypedEventHandler::new(
            move |_sender, args: *mut WebViewControlScriptNotifyEventArgs| {
                let args = unsafe { &mut *args };
                let value = args.get_value().map(|s| s.to_string())?;
                f.0(value);
                Ok(())
            },
        ))
    }
}
