//! The `wry` half of the probe.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tao::event::{Event, StartCause};
use tao::event_loop::{ControlFlow, EventLoopBuilder};
use tao::window::WindowBuilder;
use wry::{PageLoadEvent, WebViewBuilder};

use crate::server::{COOKIE_VALUE, Server};

/// Runs the wry probe and prints its report.
pub fn run(url: &str, persistent: bool, server: &Server) {
    let server = server.clone();
    let events: Arc<Mutex<Vec<String>>> = Arc::default();
    let event_loop = EventLoopBuilder::new().build();
    let window = WindowBuilder::new()
        .with_title("wry probe")
        .build(&event_loop)
        .expect("window");
    let (nav, load) = (Arc::clone(&events), Arc::clone(&events));
    let webview = WebViewBuilder::new()
        .with_incognito(!persistent)
        .with_url(url)
        .with_navigation_handler(move |u| {
            nav.lock().expect("events").push(format!("navigation-handler {u}"));
            true
        })
        .with_on_page_load_handler(move |event, u| {
            let kind = match event {
                PageLoadEvent::Started => "started",
                PageLoadEvent::Finished => "finished",
            };
            load.lock().expect("events").push(format!("page-load {kind} {u}"));
        })
        .build(&window)
        .expect("webview");

    let started = Instant::now();
    let mut stage = 0;
    event_loop.run(move |event, _, flow| {
        *flow = ControlFlow::WaitUntil(Instant::now() + Duration::from_millis(250));
        if !matches!(event, Event::NewEvents(StartCause::ResumeTimeReached { .. })) {
            return;
        }
        let finished = |n: usize| {
            events.lock().expect("events").iter().filter(|e| e.starts_with("page-load finished")).count() >= n
        };
        if stage == 0 && finished(1) {
            stage = 1;
            let cookies = webview.cookies().expect("cookies");
            let found = cookies.iter().find(|c| c.name() == "tm_session");
            println!("wry cookie-read: found={} http_only={:?} value_matches={}",
                found.is_some(),
                found.and_then(|c| c.http_only()),
                found.is_some_and(|c| c.value() == COOKIE_VALUE));
            let _ = webview.evaluate_script_with_callback("document.title", |t| {
                println!("wry js-visible-cookie: {t}");
            });
            let _ = webview.load_url("http://127.0.0.1:17891/next");
        }
        if stage == 1 && finished(2) {
            stage = 2;
            println!("wry events:");
            for e in events.lock().expect("events").iter() {
                println!("  {e}");
            }
            println!("wry server-saw-cookie-in-request={} (persistent={persistent})", server.saw_cookie());
            *flow = ControlFlow::Exit;
        }
        if started.elapsed() > Duration::from_secs(20) {
            println!("wry timeout; events={:?}; server-saw-cookie={}", events.lock().expect("events"), server.saw_cookie());
            *flow = ControlFlow::Exit;
        }
    });
}
