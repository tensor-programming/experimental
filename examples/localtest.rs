use std::borrow::Cow;
use std::io::Read;

use winit::event::{Event, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::platform::desktop::EventLoopExtDesktop;
use winit::window::WindowBuilder;

use edge_webview::edge::{self, runtime_context, Control, Process, WebView};
use edge_webview::edge_winit::{new_control, HwndType};

fn main() {
    let _rt = runtime_context();

    if !edge::is_available() {
        panic!("EdgeHTML control is not available!");
    }
    let mut args = std::env::args();

    args.next();
    let url: Cow<str> = args
        .next()
        .map(|url| url.into())
        .unwrap_or("resources/vanillajs/index.html".into());
    println!("Opening a web view to {}", url);

    let mut event_loop = EventLoop::new();

    let window = WindowBuilder::new()
        .with_title("Edge Webview")
        .build(&event_loop)
        .unwrap();

    let process = Process::new();
    let control = new_control(
        &process,
        &window,
        HwndType::FillWindow,
        None,
        None,
        Some(move |control: Control| {
            println!("Control created!");
            control.navigate_to_string(&url).unwrap();
        }),
    )
    .unwrap();

    event_loop.run_return(|event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        match event {
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                *control_flow = ControlFlow::Exit;
            }
            Event::WindowEvent {
                event: WindowEvent::Resized(size),
                ..
            } => {
                let size: (u32, u32) = size.to_physical(window.hidpi_factor()).into();
                #[warn(unused_must_use)]
                control.resize(None, Some((size.0 as i32, size.1 as i32)));
            }
            _ => (),
        }
    });
}
