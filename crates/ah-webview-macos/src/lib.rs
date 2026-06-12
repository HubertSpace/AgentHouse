//! macOS WKWebView backend for AgentHouse browser integration.
//!
//! Uses system-native WebKit via objc2 bindings.
//! All WKWebView operations are dispatched to the main thread via
//! DispatchQueue.main, since WebKit requires main-thread access.

use std::cell::RefCell;
use std::sync::mpsc;

use ah_web::WebError;

use block2::StackBlock;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{class, msg_send, AnyThread, MainThreadOnly};
use objc2_app_kit::{NSBitmapImageFileType, NSBitmapImageRep, NSImage, NSView, NSWindow};
use objc2_foundation::{MainThreadMarker, NSDictionary, NSPoint, NSRect, NSSize, NSString, NSURL};
use objc2_web_kit::{WKWebView, WKWebViewConfiguration, WKWebpagePreferences, WKWebsiteDataStore};

// ─── Safari User-Agent ──────────────────────────────────────────────

const SAFARI_UA: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.5 Safari/605.1.15";

// ─── Types ──────────────────────────────────────────────────────────

/// Wrapper to make Retained<WKWebView> Send.
/// SAFETY: All operations dispatch to main thread via on_main_thread.
#[derive(Clone)]
struct SendWebView(Retained<WKWebView>);
unsafe impl Send for SendWebView {}
unsafe impl Sync for SendWebView {}
impl std::ops::Deref for SendWebView {
    type Target = Retained<WKWebView>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl std::fmt::Debug for SendWebView {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SendWebView(..)")
    }
}

/// Wrapper to make Retained<NSWindow> Send.
/// SAFETY: All operations dispatch to main thread via on_main_thread.
#[derive(Clone)]
struct SendWindow(Retained<NSWindow>);
unsafe impl Send for SendWindow {}
unsafe impl Sync for SendWindow {}
impl std::fmt::Debug for SendWindow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SendWindow(..)")
    }
}

#[derive(Clone, Debug)]
pub enum NavigationEvent {
    Started { url: String },
    Finished { url: String, title: Option<String> },
    Failed { url: String, error: String },
}

#[derive(Clone, Debug)]
pub struct WebViewSnapshot {
    pub png_data: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// Cross-platform WebView lifecycle management trait.
pub trait WebViewProvider: Send + std::fmt::Debug {
    fn current_url(&self) -> String;
    fn title(&self) -> Option<String>;
    fn navigate(&mut self, url: &str) -> Result<(), WebError>;
    fn go_back(&mut self) -> Result<(), WebError>;
    fn go_forward(&mut self) -> Result<(), WebError>;
    fn reload(&mut self) -> Result<(), WebError>;
    fn evaluate_js(&mut self, expression: &str) -> Result<Option<String>, WebError>;
    fn screenshot(&mut self) -> Result<WebViewSnapshot, WebError>;
    fn resize(&mut self, width: u32, height: u32) -> Result<(), WebError>;
    fn set_visible(&mut self, visible: bool);
    fn is_loaded(&self) -> bool;
    fn drain_navigation_events(&mut self) -> Vec<NavigationEvent>;
    fn destroy(&mut self);
}

// ─── Shared Process Pool ────────────────────────────────────────────

thread_local! {
    static SHARED_POOL: RefCell<Option<Retained<AnyObject>>> = RefCell::new(None);
}

fn shared_process_pool() -> Retained<AnyObject> {
    SHARED_POOL.with(|shared_pool| {
        let mut shared_pool = shared_pool.borrow_mut();
        if let Some(pool) = shared_pool.as_ref() {
            return pool.clone();
        }

        let pool = unsafe {
            let pool: Retained<AnyObject> = msg_send![class!(WKProcessPool), new];
            pool
        };
        *shared_pool = Some(pool.clone());
        pool
    })
}

// ─── State tracking ─────────────────────────────────────────────────

#[derive(Debug, Default)]
struct NavigationState {
    events: Vec<NavigationEvent>,
    last_url: String,
    last_title: Option<String>,
    is_loaded: bool,
}

// ─── Main-thread dispatch helper ────────────────────────────────────

// libdispatch FFI via C shim (dispatch_get_main_queue is a macro on macOS).
extern "C" {
    fn ah_dispatch_get_main_queue() -> *mut std::ffi::c_void;
    fn ah_dispatch_sync_f(
        queue: *mut std::ffi::c_void,
        context: *mut std::ffi::c_void,
        work: unsafe extern "C" fn(*mut std::ffi::c_void),
    );
    fn ah_dispatch_async_f(
        queue: *mut std::ffi::c_void,
        context: *mut std::ffi::c_void,
        work: unsafe extern "C" fn(*mut std::ffi::c_void),
    );
}

/// Context for dispatch_sync_f callback.
struct MainThreadContext<F, R> {
    f: Option<F>,
    result: Option<R>,
}

/// Run a closure on the main thread synchronously and return the result.
/// Uses libdispatch C API to avoid Rust block Send issues.
fn on_main_thread<F, R>(f: F) -> R
where
    F: FnOnce() -> R + Send,
    R: Send,
{
    if unsafe { msg_send![class!(NSThread), isMainThread] } {
        return f();
    }

    let mut ctx = Box::new(MainThreadContext {
        f: Some(f),
        result: None,
    });
    let ctx_ptr = Box::as_mut(&mut ctx) as *mut _ as *mut std::ffi::c_void;

    unsafe extern "C" fn invoke<F, R>(ctx_ptr: *mut std::ffi::c_void)
    where
        F: FnOnce() -> R + Send,
        R: Send,
    {
        let ctx = &mut *(ctx_ptr as *mut MainThreadContext<F, R>);
        let f = ctx.f.take().unwrap();
        ctx.result = Some(f());
    }

    unsafe {
        ah_dispatch_sync_f(ah_dispatch_get_main_queue(), ctx_ptr, invoke::<F, R>);
    }

    // ctx is still valid — dispatch_sync is synchronous
    ctx.result.take().unwrap()
}

// ─── WKWebView Provider ─────────────────────────────────────────────

#[derive(Debug)]
pub struct WKWebViewProvider {
    webview: SendWebView,
    /// Hidden offscreen window that hosts the WKWebView so it actually renders.
    /// Without being in a window, WebKit's rendering pipeline won't activate.
    window: SendWindow,
    nav_state: RefCell<NavigationState>,
    destroyed: bool,
}

impl WKWebViewProvider {
    /// Create a new WKWebView. **Must be called on the main thread.**
    pub fn new() -> Result<Self, String> {
        // WKWebView must be created on main thread
        if !unsafe { msg_send![class!(NSThread), isMainThread] } {
            return Err("WKWebView must be created on the main thread".to_string());
        }

        let mtm = MainThreadMarker::new().ok_or("WKWebView requires main thread")?;

        let config = unsafe { WKWebViewConfiguration::new(mtm) };

        // Shared process pool for cookie sharing across tabs
        #[allow(deprecated)]
        unsafe {
            let pool = shared_process_pool();
            let _: () = msg_send![&config, setProcessPool: &*pool];
        }

        // Persistent data store for cookies/localStorage
        let data_store = unsafe { WKWebsiteDataStore::defaultDataStore(mtm) };
        unsafe { config.setWebsiteDataStore(&data_store) };

        // Enable JavaScript
        let webpage_prefs: Retained<WKWebpagePreferences> =
            unsafe { msg_send![class!(WKWebpagePreferences), new] };
        unsafe {
            webpage_prefs.setAllowsContentJavaScript(true);
            config.setDefaultWebpagePreferences(Some(&webpage_prefs));
        }

        // Create WKWebView
        let rect = NSRect::new(
            objc2_foundation::NSPoint::new(0.0, 0.0),
            NSSize::new(1280.0, 720.0),
        );
        let webview =
            unsafe { WKWebView::initWithFrame_configuration(WKWebView::alloc(mtm), rect, &config) };

        // Safari UA for compatibility
        unsafe { webview.setCustomUserAgent(Some(&NSString::from_str(SAFARI_UA))) };

        // Create an offscreen window to host the WKWebView.
        // WebKit requires the webview to be in a "visible" window for rendering.
        // We use a titled window positioned far offscreen — visible to the window
        // server but invisible to the user. Do NOT orderOut or WebKit won't render.
        let window = unsafe {
            let win: Retained<NSWindow> = msg_send![
                NSWindow::alloc(mtm),
                initWithContentRect: rect,
                styleMask: 1u64,   // NSWindowStyleMaskTitled — minimal but functional
                backing: 2u64,     // NSBackingStoreBuffered
                defer: false
            ];
            // Set the WKWebView as the window's content view
            let _: () = msg_send![&win, setContentView: &*webview];
            // Position far offscreen so the window is invisible to the user
            let offscreen = NSPoint::new(-32000.0, -32000.0);
            let _: () = msg_send![&win, setFrameOrigin: offscreen];
            // Do NOT orderOut — must stay in window server for WebKit rendering
            win
        };

        Ok(Self {
            webview: SendWebView(webview),
            window: SendWindow(window),
            nav_state: RefCell::new(NavigationState::default()),
            destroyed: false,
        })
    }

    /// Get the underlying NSView for embedding into a GPUI window.
    pub fn ns_view(&self) -> &NSView {
        &*self.webview.0
    }

    /// Get the hidden host window (for advanced embedding scenarios).
    pub fn host_window(&self) -> &NSWindow {
        &*self.window.0
    }
}

impl WebViewProvider for WKWebViewProvider {
    fn current_url(&self) -> String {
        if self.destroyed {
            return String::new();
        }
        let webview = self.webview.clone();
        on_main_thread(move || unsafe {
            webview
                .URL()
                .map(|u| {
                    u.absoluteString()
                        .map(|s| s.to_string())
                        .unwrap_or_default()
                })
                .unwrap_or_default()
        })
    }

    fn title(&self) -> Option<String> {
        self.nav_state.borrow().last_title.clone()
    }

    fn navigate(&mut self, url: &str) -> Result<(), WebError> {
        if self.destroyed {
            return Err(WebError::Backend("destroyed".into()));
        }

        let url_string = url.to_string(); // Pass raw String, create ObjC types on main thread
        let webview = self.webview.clone();

        self.nav_state.borrow_mut().is_loaded = false;

        on_main_thread(move || {
            let url_ns = NSString::from_str(&url_string);
            let ns_url = NSURL::URLWithString(&url_ns)
                .ok_or_else(|| WebError::Backend("invalid URL".to_string()))?;
            let request = objc2_foundation::NSURLRequest::requestWithURL(&ns_url);
            unsafe { webview.loadRequest(&request) };
            Ok(())
        })
    }

    fn go_back(&mut self) -> Result<(), WebError> {
        if self.destroyed {
            return Err(WebError::Backend("destroyed".into()));
        }
        let webview = self.webview.clone();
        on_main_thread(move || {
            unsafe { webview.goBack() };
            Ok(())
        })
    }

    fn go_forward(&mut self) -> Result<(), WebError> {
        if self.destroyed {
            return Err(WebError::Backend("destroyed".into()));
        }
        let webview = self.webview.clone();
        on_main_thread(move || {
            unsafe { webview.goForward() };
            Ok(())
        })
    }

    fn reload(&mut self) -> Result<(), WebError> {
        if self.destroyed {
            return Err(WebError::Backend("destroyed".into()));
        }
        let webview = self.webview.clone();
        self.nav_state.borrow_mut().is_loaded = false;
        on_main_thread(move || {
            unsafe { webview.reload() };
            Ok(())
        })
    }

    fn evaluate_js(&mut self, expression: &str) -> Result<Option<String>, WebError> {
        if self.destroyed {
            return Err(WebError::Backend("destroyed".into()));
        }

        let expr_ns = NSString::from_str(expression);
        let webview = self.webview.clone();
        let (tx, rx) = mpsc::channel::<Result<Option<String>, String>>();

        struct JsContext {
            webview: Option<SendWebView>,
            expr: Option<Retained<NSString>>,
            tx: Option<mpsc::Sender<Result<Option<String>, String>>>,
        }
        let ctx = Box::new(JsContext {
            webview: Some(webview),
            expr: Some(expr_ns),
            tx: Some(tx),
        });
        let ctx_ptr = Box::into_raw(ctx) as *mut std::ffi::c_void;

        unsafe extern "C" fn js_invoke(ctx_ptr: *mut std::ffi::c_void) {
            let mut ctx = Box::from_raw(ctx_ptr as *mut JsContext);
            let webview = ctx.webview.take().unwrap().0;
            let expr = ctx.expr.take().unwrap();
            let tx = ctx.tx.take().unwrap();

            // The completion block sends result directly to the outer channel.
            // We do NOT block the main thread waiting for the callback.
            let block = StackBlock::new(
                move |result: *mut AnyObject, error: *mut objc2_foundation::NSError| {
                    let res = unsafe {
                        if !error.is_null() {
                            let desc: Retained<NSString> = msg_send![error, localizedDescription];
                            Err(desc.to_string())
                        } else if result.is_null() {
                            Ok(None)
                        } else {
                            let desc: Retained<NSString> = msg_send![result, description];
                            Ok(Some(desc.to_string()))
                        }
                    };
                    let _ = tx.send(res);
                },
            );

            unsafe {
                webview.evaluateJavaScript_completionHandler(&expr, Some(&block));
            }
            // Return immediately — the block fires asynchronously on the main queue.
        }

        let is_main: bool = unsafe { msg_send![class!(NSThread), isMainThread] };

        if is_main {
            // Already on main thread: call directly, spin run loop for callback
            unsafe { js_invoke(ctx_ptr) };
            let start = std::time::Instant::now();
            let mode = NSString::from_str("kCFRunLoopDefaultMode");
            loop {
                unsafe {
                    let rl: *mut AnyObject = msg_send![class!(NSRunLoop), currentRunLoop];
                    let date: *mut AnyObject =
                        msg_send![class!(NSDate), dateWithTimeIntervalSinceNow: 0.05];
                    let _: () = msg_send![rl, runMode: &*mode, beforeDate: date];
                }
                match rx.try_recv() {
                    Ok(r) => return r.map_err(|e| WebError::Backend(format!("JS: {e}"))),
                    _ if start.elapsed() > std::time::Duration::from_secs(10) => {
                        return Err(WebError::Backend("JS eval timeout (10s)".into()));
                    }
                    _ => {}
                }
            }
        } else {
            // Background thread: dispatch async to main, wait for result
            unsafe {
                ah_dispatch_async_f(ah_dispatch_get_main_queue(), ctx_ptr, js_invoke);
            }
            match rx.recv_timeout(std::time::Duration::from_secs(10)) {
                Ok(r) => r.map_err(|e| WebError::Backend(format!("JS: {e}"))),
                Err(_) => Err(WebError::Backend("JS eval timeout (10s)".into())),
            }
        }
    }

    fn screenshot(&mut self) -> Result<WebViewSnapshot, WebError> {
        if self.destroyed {
            return Err(WebError::Backend("destroyed".into()));
        }

        let webview = self.webview.clone();
        let (tx, rx) = mpsc::channel::<Result<WebViewSnapshot, String>>();

        // Context for dispatch callback — only holds webview + sender.
        // The takeSnapshot callback sends result directly to tx; we never block the main thread.
        struct ScreenshotContext {
            webview: Option<SendWebView>,
            tx: Option<mpsc::Sender<Result<WebViewSnapshot, String>>>,
        }
        let ctx = Box::new(ScreenshotContext {
            webview: Some(webview),
            tx: Some(tx),
        });
        let ctx_ptr = Box::into_raw(ctx) as *mut std::ffi::c_void;

        unsafe extern "C" fn screenshot_invoke(ctx_ptr: *mut std::ffi::c_void) {
            let mut ctx = Box::from_raw(ctx_ptr as *mut ScreenshotContext);
            let webview = ctx.webview.take().unwrap().0;
            let tx = ctx.tx.take().unwrap();

            // The completion block sends result directly to the outer channel.
            // We do NOT block the main thread waiting for the callback.
            let block = StackBlock::new(
                move |image: *mut NSImage, error: *mut objc2_foundation::NSError| {
                    let result = unsafe {
                        if !error.is_null() {
                            let desc: Retained<NSString> = msg_send![error, localizedDescription];
                            Err(format!("Snapshot error: {}", desc))
                        } else if image.is_null() {
                            Err("No snapshot image returned".into())
                        } else {
                            // Get image dimensions
                            let size: NSSize = msg_send![image, size];
                            if size.width <= 0.0 || size.height <= 0.0 {
                                Err("Snapshot has zero dimensions".into())
                            } else {
                                // NSImage -> CGImage -> NSBitmapImageRep -> PNG
                                // (bypass TIFF which fails on offscreen webviews in GPUI)
                                let cg_image = (&*image).CGImageForProposedRect_context_hints(
                                    std::ptr::null_mut(),
                                    None,
                                    None,
                                );
                                if let Some(cg_image) = cg_image {
                                    let bitmap = NSBitmapImageRep::initWithCGImage(
                                        NSBitmapImageRep::alloc(),
                                        &cg_image,
                                    );
                                    let properties = NSDictionary::<
                                        objc2_app_kit::NSBitmapImageRepPropertyKey,
                                        AnyObject,
                                    >::new();
                                    let png = bitmap.representationUsingType_properties(
                                        NSBitmapImageFileType::PNG,
                                        &properties,
                                    );
                                    match png {
                                        Some(data) => {
                                            let png_vec = data.to_vec();
                                            Ok(WebViewSnapshot {
                                                png_data: png_vec,
                                                width: size.width as u32,
                                                height: size.height as u32,
                                            })
                                        }
                                        None => Err("Failed to encode PNG".into()),
                                    }
                                } else {
                                    Err("NSImage returned no CGImage".into())
                                }
                            }
                        }
                    };
                    let _ = tx.send(result);
                },
            );

            // Call takeSnapshotWithConfiguration:completionHandler: with nil config = full view
            unsafe {
                let _: () = msg_send![
                    &*webview,
                    takeSnapshotWithConfiguration: std::ptr::null::<AnyObject>(),
                    completionHandler: &*block
                ];
            }
            // Return immediately — the block fires asynchronously on the main queue.
        }

        let is_main: bool = unsafe { msg_send![class!(NSThread), isMainThread] };

        if is_main {
            // Already on main thread: call directly (no dispatch needed)
            unsafe { screenshot_invoke(ctx_ptr) };
            // Spin the run loop to process the async takeSnapshot callback
            let start = std::time::Instant::now();
            let mode = NSString::from_str("kCFRunLoopDefaultMode");
            loop {
                unsafe {
                    let rl: *mut AnyObject = msg_send![class!(NSRunLoop), currentRunLoop];
                    let date: *mut AnyObject =
                        msg_send![class!(NSDate), dateWithTimeIntervalSinceNow: 0.1];
                    let _: () = msg_send![rl, runMode: &*mode, beforeDate: date];
                }
                match rx.try_recv() {
                    Ok(r) => return r.map_err(|e| WebError::Backend(e)),
                    _ if start.elapsed() > std::time::Duration::from_secs(15) => {
                        return Err(WebError::Backend("Screenshot timeout (15s)".into()));
                    }
                    _ => {}
                }
            }
        } else {
            // Background thread: dispatch async to main thread, wait for result
            unsafe {
                ah_dispatch_async_f(ah_dispatch_get_main_queue(), ctx_ptr, screenshot_invoke);
            }
            // Background thread waits for the callback to send result
            match rx.recv_timeout(std::time::Duration::from_secs(15)) {
                Ok(r) => r.map_err(|e| WebError::Backend(e)),
                Err(_) => Err(WebError::Backend("Screenshot timeout (15s)".into())),
            }
        }
    }

    fn resize(&mut self, width: u32, height: u32) -> Result<(), WebError> {
        if self.destroyed {
            return Err(WebError::Backend("destroyed".into()));
        }
        let webview = self.webview.clone();
        // Encode window pointer as usize (Send+Sync) to cross thread boundary
        let window_addr = (&*self.window.0) as *const NSWindow as usize;
        on_main_thread(move || {
            let frame = NSRect::new(
                NSPoint::new(0.0, 0.0),
                NSSize::new(width as f64, height as f64),
            );
            let ns_view: &NSView = &*webview;
            ns_view.setFrame(frame);
            // Also resize the host window to match
            let win_frame = NSRect::new(
                NSPoint::new(-32000.0, -32000.0),
                NSSize::new(width as f64, height as f64),
            );
            let window_ptr = window_addr as *mut AnyObject;
            let _: () = unsafe { msg_send![window_ptr, setFrame: win_frame, display: false] };
            Ok(())
        })
    }

    fn set_visible(&mut self, visible: bool) {
        if self.destroyed {
            return;
        }
        let webview = self.webview.clone();
        on_main_thread(move || {
            let ns_view: &NSView = &*webview;
            ns_view.setHidden(!visible);
        })
    }

    fn is_loaded(&self) -> bool {
        self.nav_state.borrow().is_loaded
    }

    fn drain_navigation_events(&mut self) -> Vec<NavigationEvent> {
        // Poll state on main thread
        if !self.destroyed {
            let webview = self.webview.clone();
            let (url, title, is_loading) = on_main_thread(move || {
                let u = unsafe {
                    webview
                        .URL()
                        .map(|u| {
                            u.absoluteString()
                                .map(|s| s.to_string())
                                .unwrap_or_default()
                        })
                        .unwrap_or_default()
                };
                let t = unsafe { webview.title().map(|t| t.to_string()) };
                let loading: bool = unsafe { msg_send![&*webview, isLoading] };
                (u, t, loading)
            });

            let mut state = self.nav_state.borrow_mut();

            if !url.is_empty() && url != state.last_url {
                state
                    .events
                    .push(NavigationEvent::Started { url: url.clone() });
                state.last_url = url;
                state.is_loaded = false;
                state.last_title = None;
            }

            // Detect Finished: loading stopped AND we have a URL
            if !is_loading && !state.is_loaded && !state.last_url.is_empty() {
                // Update title if available
                if let Some(ref t) = title {
                    if !t.is_empty() {
                        state.last_title = Some(t.clone());
                    }
                }
                state.is_loaded = true;
                // Clone values before mutable borrow of events
                let finished_url = state.last_url.clone();
                let finished_title = state.last_title.clone();
                state.events.push(NavigationEvent::Finished {
                    url: finished_url,
                    title: finished_title,
                });
            }
        }

        std::mem::take(&mut self.nav_state.borrow_mut().events)
    }

    fn destroy(&mut self) {
        if self.destroyed {
            return;
        }
        self.destroyed = true;
        let webview = self.webview.clone();
        // Encode window pointer as usize (Send+Sync) to cross thread boundary
        let window_addr = (&*self.window.0) as *const NSWindow as usize;
        on_main_thread(move || {
            let ns_view: &NSView = &*webview;
            ns_view.removeFromSuperview();
            // Close the host window
            let window_ptr = window_addr as *mut AnyObject;
            let _: () = unsafe { msg_send![window_ptr, close] };
        });
    }
}

impl Drop for WKWebViewProvider {
    fn drop(&mut self) {
        self.destroy();
    }
}

unsafe impl Send for WKWebViewProvider {}
