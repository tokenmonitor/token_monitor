//! The `objc2-web-kit` half of the probe: a bare `WKWebView`.

use std::ptr::NonNull;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_app_kit::NSApplication;
use objc2_foundation::{
    NSArray, NSDate, NSHTTPCookie, NSObject, NSObjectProtocol, NSRect, NSRunLoop, NSString,
    NSURL, NSURLRequest, NSDefaultRunLoopMode,
};
use objc2_web_kit::{
    WKNavigation, WKNavigationAction, WKNavigationActionPolicy, WKNavigationDelegate, WKWebView,
    WKWebViewConfiguration, WKWebsiteDataStore,
};

use crate::server::{COOKIE_VALUE, Server};

type Log = Arc<Mutex<Vec<String>>>;

struct Ivars {
    log: Log,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "ProbeNavDelegate"]
    #[ivars = Ivars]
    struct NavDelegate;

    unsafe impl NSObjectProtocol for NavDelegate {}

    unsafe impl WKNavigationDelegate for NavDelegate {
        #[unsafe(method(webView:didStartProvisionalNavigation:))]
        fn did_start(&self, _wv: &WKWebView, _nav: Option<&WKNavigation>) {
            self.ivars().log.lock().expect("log").push("didStartProvisionalNavigation".into());
        }

        #[unsafe(method(webView:didFinishNavigation:))]
        fn did_finish(&self, _wv: &WKWebView, _nav: Option<&WKNavigation>) {
            self.ivars().log.lock().expect("log").push("didFinishNavigation".into());
        }

        #[unsafe(method(webView:decidePolicyForNavigationAction:decisionHandler:))]
        fn decide(
            &self,
            _wv: &WKWebView,
            action: &WKNavigationAction,
            handler: &block2::DynBlock<dyn Fn(WKNavigationActionPolicy)>,
        ) {
            let url = unsafe { action.request().URL() }
                .and_then(|u| u.absoluteString())
                .map(|s| s.to_string())
                .unwrap_or_default();
            self.ivars().log.lock().expect("log").push(format!("decidePolicyForNavigationAction {url}"));
            handler.call((WKNavigationActionPolicy::Allow,));
        }
    }
);

impl NavDelegate {
    fn new(mtm: MainThreadMarker, log: Log) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(Ivars { log });
        unsafe { msg_send![super(this), init] }
    }
}

fn spin(seconds: f64) {
    let run_loop = NSRunLoop::currentRunLoop();
    let until = NSDate::dateWithTimeIntervalSinceNow(seconds);
    unsafe {
        let _ = run_loop.runMode_beforeDate(NSDefaultRunLoopMode, &until);
    }
}

fn wait_until(limit: Duration, mut done: impl FnMut() -> bool) -> bool {
    let start = Instant::now();
    while start.elapsed() < limit {
        if done() {
            return true;
        }
        spin(0.05);
    }
    done()
}

/// Runs the WebKit probe and prints its report.
pub fn run(url: &str, persistent: bool, server: &Server) {
    let mtm = MainThreadMarker::new().expect("main thread");
    let _app = NSApplication::sharedApplication(mtm);
    let log: Log = Arc::default();
    let delegate = NavDelegate::new(mtm, Arc::clone(&log));
    let config = unsafe { WKWebViewConfiguration::new(mtm) };
    let store = unsafe {
        if persistent {
            WKWebsiteDataStore::defaultDataStore(mtm)
        } else {
            WKWebsiteDataStore::nonPersistentDataStore(mtm)
        }
    };
    unsafe { config.setWebsiteDataStore(&store) };
    let webview = unsafe {
        WKWebView::initWithFrame_configuration(WKWebView::alloc(mtm), NSRect::ZERO, &config)
    };
    unsafe { webview.setNavigationDelegate(Some(ProtocolObject::from_ref(&*delegate))) };
    let load = |u: &str| unsafe {
        let ns = NSString::from_str(u);
        if let Some(parsed) = NSURL::URLWithString(&ns) {
            let _ = webview.loadRequest(&NSURLRequest::requestWithURL(&parsed));
        }
    };
    let finished = |n: usize| log.lock().expect("log").iter().filter(|e| *e == "didFinishNavigation").count() >= n;
    load(url);
    if !wait_until(Duration::from_secs(10), || finished(1)) {
        println!("webkit timeout loading the first page; log={:?}", log.lock().expect("log"));
        return;
    }

    let cookies: Arc<Mutex<Option<Vec<(String, String, bool)>>>> = Arc::default();
    let sink = Arc::clone(&cookies);
    let block = RcBlock::new(move |all: NonNull<NSArray<NSHTTPCookie>>| {
        let all = unsafe { all.as_ref() };
        let rows = all
            .to_vec()
            .iter()
            .map(|c| (c.name().to_string(), c.value().to_string(), c.isHTTPOnly()))
            .collect();
        *sink.lock().expect("sink") = Some(rows);
    });
    unsafe { store.httpCookieStore().getAllCookies(&block) };
    wait_until(Duration::from_secs(5), || cookies.lock().expect("sink").is_some());
    let rows = cookies.lock().expect("sink").clone().unwrap_or_default();
    let found = rows.iter().find(|(n, _, _)| n == "tm_session");
    println!(
        "webkit cookie-read: found={} http_only={:?} value_matches={}",
        found.is_some(),
        found.map(|c| c.2),
        found.is_some_and(|c| c.1 == COOKIE_VALUE)
    );
    let js = Arc::new(Mutex::new(None::<String>));
    let js_sink = Arc::clone(&js);
    let js_block = RcBlock::new(move |value: *mut objc2::runtime::AnyObject, _err: *mut objc2_foundation::NSError| {
        let text = if value.is_null() { "<none>".to_owned() } else { unsafe { format!("{:?}", &*value) } };
        *js_sink.lock().expect("js") = Some(text);
    });
    unsafe {
        webview.evaluateJavaScript_completionHandler(&NSString::from_str("document.title"), Some(&js_block));
    }
    wait_until(Duration::from_secs(3), || js.lock().expect("js").is_some());
    println!("webkit js-visible-cookie: {:?}", js.lock().expect("js"));

    load("http://127.0.0.1:17891/next");
    wait_until(Duration::from_secs(10), || finished(2));
    println!("webkit events:");
    for e in log.lock().expect("log").iter() {
        println!("  {e}");
    }
    println!("webkit server-saw-cookie-in-request={} (persistent={persistent})", server.saw_cookie());
}
