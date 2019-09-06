use std::cell::{Cell, RefCell};
use std::ffi::{c_void, CStr, CString};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::{panic, process};

thread_local! {
    static MAIN_THREAD: Cell<bool> = Cell::new(false);
}

static INITIALIZED: AtomicBool = AtomicBool::new(false);

pub trait Handler: 'static {
    fn handle(&mut self, window: Window, message: &str) {
        let _ = (window, message);
    }
}

impl<F: FnMut(Window, &str) + 'static> Handler for F {
    fn handle(&mut self, window: Window, message: &str) {
        (self)(window, message)
    }
}

#[derive(Clone)]
pub struct Window {
    data: Rc<RefCell<Option<raw::webview>>>,
}

type Data = (Window, Box<dyn Handler>);

impl Window {
    pub fn new(opts: Options) -> Self {
        assert_main();

        let this = Window {
            data: Rc::new(RefCell::new(None)),
        };

        let handler = opts.handler.unwrap_or(Box::new(|_, _: &_| {}));

        let opts = raw::webview_options {
            initial_width: opts.initial_width,
            initial_height: opts.initial_height,
            minimum_width: opts.minimum_width,
            minimum_height: opts.minimum_height,

            borderless: opts.borderless,
            debug: opts.debug,

            data: Box::<Data>::into_raw(Box::new((this.clone(), handler))) as _,
            closed: Some(closed),
            message: Some(message),
        };

        let raw = unsafe { raw::webview_new(opts) };
        this.data.replace(Some(raw));

        unsafe extern "C" fn closed(data: *mut c_void) {
            abort_on_panic(|| {
                let _ = Box::<Data>::from_raw(data as _);
            });
        }

        unsafe extern "C" fn message(data: *mut c_void, message: *const i8) {
            abort_on_panic(|| {
                let data = data as *mut Data;

                match CStr::from_ptr(message).to_str() {
                    Ok(message) => {
                        (*data).1.handle((*data).0.clone(), message);
                    }
                    Err(e) => {}
                }
            });
        }

        this
    }

    pub fn with_handler(handler: impl Handler) -> Self {
        Self::new(Options {
            handler: Some(Box::new(handler)),
            ..Default::default()
        })
    }

    pub fn eval<I: Into<String>>(&self, s: I) {
        if let Some(data) = *self.data.borrow_mut() {
            let s = string_to_cstring(s);
            unsafe {
                raw::webview_eval(data, s.as_ptr());
            }
        }
    }

    pub fn load<I: Into<String>>(&self, s: I) {
        if let Some(data) = *self.data.borrow_mut() {
            let s = string_to_cstring(s);
            unsafe {
                raw::webview_load(data, s.as_ptr());
            }
        }
    }

    pub fn title<I: Into<String>>(&self, s: I) {
        if let Some(data) = *self.data.borrow_mut() {
            let s = string_to_cstring(s);
            unsafe {
                raw::webview_title(data, s.as_ptr());
            }
        }
    }

    pub fn focus(&self) {
        if let Some(data) = *self.data.borrow_mut() {
            unsafe {
                raw::webview_focus(data);
            }
        }
    }

    pub fn close(&self) {
        if let Some(data) = *self.data.borrow_mut() {
            unsafe {
                raw::webview_close(data);
            }
        }
    }
}

impl Default for Window {
    fn default() -> Self {
        Self::new(Default::default())
    }
}

pub struct Options {
    pub initial_width: usize,
    pub initial_height: usize,
    pub minimum_width: usize,
    pub minimum_height: usize,

    pub borderless: bool,
    pub debug: bool,

    pub handler: Option<Box<dyn Handler>>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            initial_width: 640,
            initial_height: 480,
            minimum_width: 480,
            minimum_height: 360,

            borderless: false,
            debug: true,

            handler: None,
        }
    }
}

pub unsafe fn start(cb: fn()) {
    static mut INIT: Option<fn()> = None;
    INIT = Some(cb);

    unsafe extern "C" fn init() {
        abort_on_panic(|| {
            MAIN_THREAD.with(|initialized| {
                initialized.set(true);
            });

            INITIALIZED.store(true, Ordering::Relaxed);

            INIT.unwrap()();
        });
    }

    raw::webview_start(Some(init));
}

pub fn exit() {
    assert_main();

    unsafe {
        raw::webview_exit();
    }
}

pub fn dispatch<F: FnOnce() + Send>(f: F) {
    assert_initialized();

    unsafe {
        raw::webview_dispatch(Box::<F>::into_raw(Box::new(f)) as _, Some(execute::<F>));
    }

    unsafe extern "C" fn execute<F: FnOnce() + Send>(data: *mut c_void) {
        abort_on_panic(|| {
            Box::<F>::from_raw(data as _)();
        });
    }
}

fn abort_on_panic<F: FnOnce() + panic::UnwindSafe>(f: F) {
    if panic::catch_unwind(f).is_err() {
        process::abort();
    }
}

fn assert_initialized() {
    assert!(INITIALIZED.load(Ordering::Relaxed));
}

fn assert_main() {
    MAIN_THREAD.with(|initialized| {
        assert!(initialized.get());
    });
}

fn string_to_cstring<I: Into<String>>(s: I) -> CString {
    CString::new(s.into()).unwrap()
}

mod raw {
    #![allow(dead_code, nonstandard_style)]
    include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
}
