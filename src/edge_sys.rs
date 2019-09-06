use include_dir::Dir;
use webview_sys as ffi;

use ffi::*;
use std::ffi::{CStr, CString};
use std::fmt;
use std::marker::PhantomData;
use std::os::raw::*;
use std::path::Path;
use std::ptr;

pub enum Content<'a, S: Into<String>> {
    Html(S),
    Url(S),
    Dir(Dir<'a>, S),
}

pub enum Event {
    Quit,
    DOMContentLoaded,
    ScriptNotify(String),
}

#[derive(Debug)]
pub enum Error {
    Null(std::ffi::NulError),
    Runtime(i32, String),
}

impl From<std::ffi::NulError> for Error {
    fn from(err: std::ffi::NulError) -> Error {
        Error::Null(err)
    }
}

impl std::error::Error for Error {
    fn description(&self) -> &str {
        match *self {
            Error::Null(ref err) => err.description(),
            Error::Runtime(_, ref message) => message.as_str(),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Error::Null(ref err) => write!(f, "Null error: {}", err),
            Error::Runtime(code, ref message) => {
                write!(f, "Windows Runtime error 0x{:08x}: \"{}\"", code, message)
            }
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;

pub struct WebView<'a> {
    window: *mut c_void,
    internal: Box<InternalData<'a>>,
}

struct InternalData<'a> {
    dir: Option<include_dir::Dir<'a>>,
}

pub struct Dispatcher<'a> {
    phantom: PhantomData<&'a WebView<'a>>,
    window: *mut c_void,
    webview: *mut c_void,
}

struct CallbackInfo<'a> {
    callback: Box<FnMut(&'a mut WebView<'a>) + 'a>,
}

pub struct EventIterator<'a> {
    phantom: PhantomData<&'a WebView<'a>>,
    window: *mut c_void,
    blocking: bool,
}

impl<'a> WebView<'a> {
    pub fn new<S: Into<String>>(
        title: &str,
        content: Content<'a, S>,
        size: (i32, i32),
        resizable: bool,
    ) -> Result<WebView<'a>> {
        let title = CString::new(title)?;
        let window = ffi_result(unsafe {
            let mut window: *mut c_void = ptr::null_mut();
            let result = webview_new(title.as_ptr(), size.0, size.1, resizable, &mut window);
            (window, result)
        })?;

        let mut webview = WebView {
            window,
            internal: Box::new(InternalData { dir: None }),
        };
        let internal = webview.internal.as_mut() as *mut InternalData as *mut c_void;

        ffi_result(unsafe {
            match content {
                Content::Url(url) => {
                    let url = CString::new(url.into())?;
                    let result = webview_navigate(window, internal, url.as_ptr(), ContentType_Url);
                    ((), result)
                }
                Content::Html(html) => {
                    let html = CString::new(html.into())?;
                    let result =
                        webview_navigate(window, internal, html.as_ptr(), ContentType_Html);
                    ((), result)
                }
                Content::Dir(dir, source) => {
                    webview.internal.dir = Some(dir);
                    let source = CString::new(source.into())?;
                    let result =
                        webview_navigate_with_streamresolver(window, internal, source.as_ptr());
                    ((), result)
                }
            }
        })?;

        Ok(webview)
    }

    pub fn dispatcher(&mut self) -> Dispatcher<'a> {
        Dispatcher {
            phantom: PhantomData,
            window: self.window,
            webview: self as *mut WebView as *mut c_void,
        }
    }

    pub fn poll_iter(&self) -> EventIterator<'a> {
        EventIterator {
            phantom: PhantomData,
            window: self.window,
            blocking: false,
        }
    }

    pub fn wait_iter(&self) -> EventIterator<'a> {
        EventIterator {
            phantom: PhantomData,
            window: self.window,
            blocking: true,
        }
    }

    pub fn eval_script(&mut self, script: &str) -> Result<String> {
        let script = CString::new(script)?;

        let ret = ffi_result(unsafe {
            let mut ret: *mut c_char = ptr::null_mut();
            let result = webview_eval_script(self.window, script.as_ptr(), &mut ret);
            (ret, result)
        })?;

        let value = unsafe {
            let value = CStr::from_ptr(ret).to_string_lossy().into_owned();
            webview_string_free(ret);
            value
        };
        Ok(value)
    }

    pub fn inject_css(&mut self, css: &str) -> Result<()> {
        let css = CString::new(css)?;
        ffi_result(unsafe {
            let result = webview_inject_css(self.window, css.as_ptr());
            ((), result)
        })
    }
}

impl<'a> Drop for WebView<'a> {
    fn drop(&mut self) {
        unsafe { webview_free(self.window) };
    }
}

fn ffi_result<T>(result: (T, i32)) -> Result<T> {
    match result {
        (value, 0) => Ok(value),
        (_, code) => {
            let mut msg: *mut c_char = ptr::null_mut();
            unsafe {
                webview_get_error_message(&mut msg);
            };

            let message = unsafe { CStr::from_ptr(msg).to_string_lossy().into_owned() };
            unsafe { webview_string_free(msg) };

            Err(Error::Runtime(code, message))
        }
    }
}

unsafe impl<'a> Send for Dispatcher<'a> {}
unsafe impl<'a> Sync for Dispatcher<'a> {}

impl<'a> Dispatcher<'a> {
    pub fn dispatch<F>(&mut self, callback: F) -> Result<()>
    where
        F: FnMut(&mut WebView) + 'a,
    {
        ffi_result(unsafe {
            let info_ptr = Box::into_raw(Box::new(CallbackInfo {
                callback: Box::new(callback),
            }));
            let result = webview_dispatch(self.window, self.webview, info_ptr as *mut c_void);
            ((), result)
        })
    }
}

impl<'a> Clone for Dispatcher<'a> {
    fn clone(&self) -> Dispatcher<'a> {
        Dispatcher {
            phantom: self.phantom,
            window: self.window,
            webview: self.webview,
        }
    }
}

impl<'a> Iterator for EventIterator<'a> {
    type Item = Event;

    fn next(&mut self) -> Option<Event> {
        let mut event: u32 = EventType_None;
        let mut data: *mut c_char = ptr::null_mut();

        unsafe { webview_loop(self.window, self.blocking, &mut event, &mut data) };

        match event {
            EventType_Quit => Some(Event::Quit),
            EventType_DOMContentLoaded => Some(Event::DOMContentLoaded),
            EventType_ScriptNotify => {
                let response = unsafe { CStr::from_ptr(data).to_string_lossy().to_string() };
                unsafe { webview_string_free(data) };
                Some(Event::ScriptNotify(response))
            }
            _ => None,
        }
    }
}

pub fn webview<'a, S: Into<String>, F>(
    title: &str,
    content: Content<S>,
    size: (i32, i32),
    resizable: bool,
    mut callback: F,
) -> Result<()>
where
    F: FnMut(&mut WebView, Event) + 'a,
{
    let mut webview = WebView::new(title, content, size, resizable)?;

    'running: loop {
        for event in webview.wait_iter() {
            match event {
                Event::Quit => {
                    break 'running;
                }
                event => {
                    callback(&mut webview, event);
                }
            }
        }
    }

    Ok(())
}

#[no_mangle]
pub extern "C" fn webview_get_content(
    webview_ptr: *mut c_void,
    source: *const c_char,
    content: *mut *const u8,
    length: *mut usize,
) -> bool {
    let internal = unsafe { (webview_ptr as *mut InternalData).as_mut().unwrap() };
    unsafe {
        *content = ptr::null();
        *length = 0;
    };

    if let Some(ref dir) = internal.dir {
        let source = unsafe { CStr::from_ptr(source).to_str().unwrap() };
        let path = Path::new(source);
        let path = if path.starts_with("/") {
            path.strip_prefix("/").unwrap()
        } else {
            path
        };
        if dir.contains(path) {
            let file = dir.get_file(path.to_str().unwrap()).unwrap();
            let body = file.contents();
            unsafe {
                *content = body.as_ptr();
                *length = body.len();
            };

            return true;
        }
    }

    false
}

#[no_mangle]
pub extern "C" fn webview_dispatch_callback(webview_ptr: *mut c_void, info_ptr: *mut c_void) {
    let mut webview = unsafe { (webview_ptr as *mut WebView).as_mut().unwrap() };
    let mut info = unsafe { Box::from_raw(info_ptr as *mut CallbackInfo) };
    (info.callback)(&mut webview);
}
