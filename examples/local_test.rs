use edge_webview::edge_manual;

#[macro_use]
extern crate include_dir;

use edge_webview::edge_manual::Content::*;
use edge_webview::edge_manual::Event::*;
use edge_webview::edge_manual::*;

use include_dir::Dir;

use std::fs::File;
use std::io::prelude::*;
use std::thread;

fn main() {
    let mut html = File::open("resources/index.html").unwrap();
    let mut html_contents = String::new();
    html.read_to_string(&mut html_contents).unwrap();
    // let mut js = File::open("resources/vue-example/dist/build.js").unwrap();
    // let mut contents = String::new();
    // js.read_to_string(&mut contents).unwrap();

    let mut webview = WebView::new(
        "Basic Example",
        Content::Html(html_contents),
        (800, 600),
        true,
    )
    .unwrap();
    let mut dispatcher = webview.dispatcher();

    let worker = thread::spawn(move || {
        dispatcher
            .dispatch(move |wv| {
                wv.eval_script("document.body.style.backgroundColor = '#0f0';")
                    .unwrap();
            })
            .unwrap();
    });

    'running: loop {
        for event in webview.poll_iter() {
            match event {
                Event::Quit => {
                    break 'running;
                }
                _ => {}
            }
        }
    }

    worker.join().unwrap();
}
