//! macOS WKWebView backend for AgentHouse browser integration.
//!
//! Uses system-native WebKit via objc2 bindings.
//! All WKWebView operations are dispatched to the main thread via
//! DispatchQueue.main, since WebKit requires main-thread access.

use std::cell::{Cell, RefCell};
use std::sync::mpsc;

use ah_web::WebError;

use block2::StackBlock;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2::{class, define_class, msg_send, AnyThread, DefinedClass, MainThreadOnly};
use objc2_app_kit::{
    NSApplication, NSAutoresizingMaskOptions, NSBezierPath, NSColor, NSCompositingOperation,
    NSImage, NSResponder, NSView, NSWindow,
};
use objc2_foundation::{
    MainThreadMarker, NSData, NSObject, NSObjectProtocol, NSPoint, NSProcessInfo, NSRect, NSSize,
    NSString, NSURL,
};
use objc2_web_kit::{
    WKNavigationAction, WKUIDelegate, WKWebView, WKWebViewConfiguration, WKWebpagePreferences,
    WKWebsiteDataStore, WKWindowFeatures,
};

// ─── Safari User-Agent ──────────────────────────────────────────────

const SAFARI_UA: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.5 Safari/605.1.15";

struct WebViewContainerIvars {
    accepts_pointer_events: Cell<bool>,
}

define_class!(
    #[unsafe(super(NSView))]
    #[thread_kind = MainThreadOnly]
    #[ivars = WebViewContainerIvars]
    struct WebViewContainer;

    impl WebViewContainer {
        #[unsafe(method(hitTest:))]
        fn hit_test(&self, point: NSPoint) -> *mut NSView {
            if self.ivars().accepts_pointer_events.get() {
                unsafe { msg_send![super(self), hitTest: point] }
            } else {
                std::ptr::null_mut()
            }
        }

        #[unsafe(method(acceptsFirstMouse:))]
        fn accepts_first_mouse(&self, _event: *mut AnyObject) -> bool {
            true
        }

        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, event: *mut AnyObject) {
            unsafe {
                let _: () = msg_send![super(self), mouseDown: event];
            }
        }
    }
);

define_class!(
    #[unsafe(super(WKWebView))]
    #[thread_kind = MainThreadOnly]
    struct AgentHouseWebView;

    impl AgentHouseWebView {
        #[unsafe(method(acceptsFirstMouse:))]
        fn accepts_first_mouse(&self, _event: *mut AnyObject) -> bool {
            true
        }

        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, event: *mut AnyObject) {
            focus_if_possible(self);
            unsafe {
                let _: () = msg_send![super(self), mouseDown: event];
            }
        }

        #[unsafe(method(rightMouseDown:))]
        fn right_mouse_down(&self, event: *mut AnyObject) {
            focus_if_possible(self);
            unsafe {
                let _: () = msg_send![super(self), rightMouseDown: event];
            }
        }

        #[unsafe(method(otherMouseDown:))]
        fn other_mouse_down(&self, event: *mut AnyObject) {
            focus_if_possible(self);
            unsafe {
                let _: () = msg_send![super(self), otherMouseDown: event];
            }
        }
    }
);

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    struct AgentHouseWebViewDelegate;

    unsafe impl NSObjectProtocol for AgentHouseWebViewDelegate {}

    unsafe impl WKUIDelegate for AgentHouseWebViewDelegate {
        #[unsafe(method_id(webView:createWebViewWithConfiguration:forNavigationAction:windowFeatures:))]
        #[unsafe(method_family = none)]
        fn create_web_view_with_configuration(
            &self,
            web_view: &WKWebView,
            _configuration: &WKWebViewConfiguration,
            navigation_action: &WKNavigationAction,
            _window_features: &WKWindowFeatures,
        ) -> Option<Retained<WKWebView>> {
            let opens_new_window = unsafe { navigation_action.targetFrame().is_none() };
            if opens_new_window {
                let request = unsafe { navigation_action.request() };
                unsafe {
                    web_view.loadRequest(&request);
                }
            }
            None
        }
    }
);

impl AgentHouseWebViewDelegate {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm);
        unsafe { msg_send![this, init] }
    }
}

impl WebViewContainer {
    fn new(frame: NSRect) -> Retained<Self> {
        let this =
            Self::alloc(MainThreadMarker::new().expect("WebViewContainer requires main thread"))
                .set_ivars(WebViewContainerIvars {
                    accepts_pointer_events: Cell::new(true),
                });
        unsafe { msg_send![super(this), initWithFrame: frame] }
    }

    fn set_accepts_pointer_events(&self, accepts: bool) {
        self.ivars().accepts_pointer_events.set(accepts);
    }
}

/// Configure the AppKit-visible app name and Dock icon for `cargo run`.
///
/// A bundled `.app` gets these from `Info.plist`, but the development
/// binary path otherwise appears as `agenthouse` and has no Dock icon.
pub fn configure_running_application(app_name: &str, icon_bytes: &'static [u8]) {
    let app_name = app_name.to_string();
    on_main_thread(move || {
        let mtm = match MainThreadMarker::new() {
            Some(mtm) => mtm,
            None => return,
        };
        let process_name = NSString::from_str(&app_name);
        NSProcessInfo::processInfo().setProcessName(&process_name);

        if icon_bytes.is_empty() {
            return;
        }
        let data =
            unsafe { NSData::dataWithBytes_length(icon_bytes.as_ptr().cast(), icon_bytes.len()) };
        if let Some(image) = NSImage::initWithData(NSImage::alloc(), &data) {
            let image = rounded_app_icon_image(&image).unwrap_or(image);
            unsafe {
                NSApplication::sharedApplication(mtm).setApplicationIconImage(Some(&image));
            }
        }
    });
}

fn rounded_app_icon_image(source: &NSImage) -> Option<Retained<NSImage>> {
    let canvas_side = 1024.0;
    let canvas = NSSize::new(canvas_side, canvas_side);
    let inset = canvas_side * 0.09;
    let rect = NSRect::new(
        NSPoint::new(inset, inset),
        NSSize::new(canvas_side - inset * 2.0, canvas_side - inset * 2.0),
    );
    let corner = rect.size.width * 0.225;
    let composed = NSImage::initWithSize(NSImage::alloc(), canvas);

    #[allow(deprecated)]
    {
        composed.lockFocus();
        NSColor::clearColor().setFill();
        NSBezierPath::bezierPathWithRect(NSRect::new(NSPoint::new(0.0, 0.0), canvas)).fill();
        let clip = NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(rect, corner, corner);
        clip.addClip();
        unsafe {
            source.drawInRect_fromRect_operation_fraction_respectFlipped_hints(
                rect,
                NSRect::new(NSPoint::new(0.0, 0.0), source.size()),
                NSCompositingOperation::Copy,
                1.0,
                false,
                None,
            );
        }
        composed.unlockFocus();
    }

    Some(composed)
}

// ─── Types ──────────────────────────────────────────────────────────

/// Wrapper to make Retained<WKWebView> Send.
/// SAFETY: All operations dispatch to main thread via on_main_thread.
struct SendWebView(Option<Retained<WKWebView>>);
unsafe impl Send for SendWebView {}
unsafe impl Sync for SendWebView {}
impl std::ops::Deref for SendWebView {
    type Target = WKWebView;
    fn deref(&self) -> &Self::Target {
        &self.0.as_ref().expect("webview should be present")
    }
}
impl Clone for SendWebView {
    fn clone(&self) -> Self {
        Self(Some(
            self.0.as_ref().expect("webview should be present").clone(),
        ))
    }
}
impl std::fmt::Debug for SendWebView {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SendWebView(..)")
    }
}
impl Drop for SendWebView {
    fn drop(&mut self) {
        let Some(webview) = self.0.take() else {
            return;
        };
        let is_main: bool = unsafe { msg_send![class!(NSThread), isMainThread] };
        if is_main {
            drop(webview);
        } else {
            let webview = SendWebView(Some(webview));
            on_main_thread(move || drop(webview));
        }
    }
}

/// Wrapper to make Retained<AgentHouseWebViewDelegate> Send.
/// SAFETY: The delegate is retained only to keep WKWebView's weak UIDelegate
/// alive. Release is dispatched to the main thread like other WebKit objects.
struct SendWebViewDelegate(Option<Retained<AgentHouseWebViewDelegate>>);
unsafe impl Send for SendWebViewDelegate {}
unsafe impl Sync for SendWebViewDelegate {}
impl std::fmt::Debug for SendWebViewDelegate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SendWebViewDelegate(..)")
    }
}
impl Drop for SendWebViewDelegate {
    fn drop(&mut self) {
        let Some(delegate) = self.0.take() else {
            return;
        };
        let is_main: bool = unsafe { msg_send![class!(NSThread), isMainThread] };
        if is_main {
            drop(delegate);
        } else {
            let delegate = SendWebViewDelegate(Some(delegate));
            on_main_thread(move || drop(delegate));
        }
    }
}

/// Wrapper to make Retained<WebViewContainer> Send.
/// SAFETY: All operations dispatch to main thread via on_main_thread.
struct SendWebViewContainer(Option<Retained<WebViewContainer>>);
unsafe impl Send for SendWebViewContainer {}
unsafe impl Sync for SendWebViewContainer {}
impl std::ops::Deref for SendWebViewContainer {
    type Target = WebViewContainer;
    fn deref(&self) -> &Self::Target {
        self.0.as_ref().expect("container should be present")
    }
}
impl Clone for SendWebViewContainer {
    fn clone(&self) -> Self {
        Self(Some(
            self.0
                .as_ref()
                .expect("container should be present")
                .clone(),
        ))
    }
}
impl std::fmt::Debug for SendWebViewContainer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SendWebViewContainer(..)")
    }
}
impl Drop for SendWebViewContainer {
    fn drop(&mut self) {
        let Some(container) = self.0.take() else {
            return;
        };
        let is_main: bool = unsafe { msg_send![class!(NSThread), isMainThread] };
        if is_main {
            drop(container);
        } else {
            let container = SendWebViewContainer(Some(container));
            on_main_thread(move || drop(container));
        }
    }
}

/// Wrapper to make Retained<NSWindow> Send.
/// SAFETY: All operations dispatch to main thread via on_main_thread.
struct SendWindow(Option<Retained<NSWindow>>);
unsafe impl Send for SendWindow {}
unsafe impl Sync for SendWindow {}
impl std::ops::Deref for SendWindow {
    type Target = NSWindow;
    fn deref(&self) -> &Self::Target {
        &self.0.as_ref().expect("window should be present")
    }
}
impl Clone for SendWindow {
    fn clone(&self) -> Self {
        Self(Some(
            self.0.as_ref().expect("window should be present").clone(),
        ))
    }
}
impl std::fmt::Debug for SendWindow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SendWindow(..)")
    }
}
impl Drop for SendWindow {
    fn drop(&mut self) {
        let Some(window) = self.0.take() else {
            return;
        };
        let is_main: bool = unsafe { msg_send![class!(NSThread), isMainThread] };
        if is_main {
            drop(window);
        } else {
            let window = SendWindow(Some(window));
            on_main_thread(move || drop(window));
        }
    }
}

#[derive(Clone, Debug)]
pub struct WKWebViewSurface {
    webview: SendWebView,
    container: SendWebViewContainer,
}

impl WKWebViewSurface {
    pub fn attach_to_view(
        &self,
        native_parent_view: usize,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        accepts_pointer_events: bool,
    ) -> Result<(), String> {
        let webview = self.webview.clone();
        let container = self.container.clone();
        on_main_thread(move || {
            if native_parent_view == 0 {
                return Err("missing native parent view".to_string());
            }

            let parent = native_parent_view as *mut NSView;
            let parent = unsafe { &*parent };
            let parent_bounds = parent.bounds();
            let container_view: &NSView = &*container;

            if !container_view.isDescendantOf(parent) {
                container_view.removeFromSuperview();
                parent.addSubview(container_view);
            }

            // GPUI reports element bounds with a top-left origin in window
            // content coordinates. The host NSView (GPUIView) is a non-flipped
            // AppKit view, so its local coordinate system is bottom-left.
            // Flip the y to land at the same visible rectangle.
            let flipped_y =
                (parent_bounds.size.height - y.max(0.0) as f64 - height.max(0.0) as f64).max(0.0);
            let frame = NSRect::new(
                NSPoint::new(x.max(0.0) as f64, flipped_y),
                NSSize::new(width.max(1.0) as f64, height.max(1.0) as f64),
            );
            let content_bounds = NSRect::new(NSPoint::new(0.0, 0.0), frame.size);
            container.set_accepts_pointer_events(accepts_pointer_events);
            container_view.setFrame(frame);
            container_view.setBounds(content_bounds);
            container_view.setAutoresizingMask(NSAutoresizingMaskOptions::ViewNotSizable);
            container_view.setHidden(false);
            container_view.setAlphaValue(1.0);
            container_view.setNeedsLayout(true);

            let webview_view: &NSView = &*webview;
            if !webview_view.isDescendantOf(container_view) {
                webview_view.removeFromSuperview();
                container_view.addSubview(webview_view);
            }
            webview_view.setFrame(content_bounds);
            webview_view.setBounds(content_bounds);
            webview_view.setAutoresizingMask(
                NSAutoresizingMaskOptions::ViewWidthSizable
                    | NSAutoresizingMaskOptions::ViewHeightSizable,
            );
            webview_view.setHidden(false);
            webview_view.setAlphaValue(1.0);
            webview_view.setNeedsLayout(true);
            let _: () = unsafe { msg_send![&*container_view, layoutSubtreeIfNeeded] };
            let _: () = unsafe { msg_send![&*webview_view, layoutSubtreeIfNeeded] };
            container_view.setNeedsDisplay(true);
            webview_view.setNeedsDisplay(true);
            Ok(())
        })
    }

    pub fn resize(&self, width: u32, height: u32) -> Result<(), String> {
        let webview = self.webview.clone();
        let container = self.container.clone();
        on_main_thread(move || {
            let ns_view: &NSView = &*container;
            let current = ns_view.frame();
            let frame = NSRect::new(
                current.origin,
                NSSize::new(width.max(1) as f64, height.max(1) as f64),
            );
            let content_bounds = NSRect::new(NSPoint::new(0.0, 0.0), frame.size);
            ns_view.setFrame(frame);
            ns_view.setBounds(content_bounds);
            ns_view.setNeedsLayout(true);
            let webview_view: &NSView = &*webview;
            webview_view.setFrame(content_bounds);
            webview_view.setBounds(content_bounds);
            webview_view.setNeedsLayout(true);
            let _: () = unsafe { msg_send![&*ns_view, layoutSubtreeIfNeeded] };
            let _: () = unsafe { msg_send![&*webview_view, layoutSubtreeIfNeeded] };
            Ok(())
        })
    }

    pub fn reapply_layout_and_dispatch_resize(&self) -> Result<(), String> {
        let webview = self.webview.clone();
        let container = self.container.clone();
        on_main_thread_async(move || {
            reapply_webview_layout(&container, &webview);
            dispatch_dom_resize_event(&webview);
        });
        Ok(())
    }

    pub fn set_accepts_pointer_events(&self, accepts: bool) {
        let container = self.container.clone();
        on_main_thread(move || {
            container.set_accepts_pointer_events(accepts);
            let ns_view: &NSView = &*container;
            ns_view.setHidden(false);
            ns_view.setAlphaValue(1.0);
        });
    }

    pub fn focus_webview(&self) {
        let webview = self.webview.clone();
        on_main_thread(move || {
            let webview_view: &NSView = &*webview;
            focus_if_possible(webview_view);
        });
    }

    pub fn release_focus(&self) {
        let container = self.container.clone();
        on_main_thread(move || {
            let ns_view: &NSView = &*container;
            // The container's superview is the GPUI host NSView (GPUIView).
            // Promote that host back to first responder instead of resigning to
            // nil — see `reclaim_first_responder_to_host` for why.
            let host = unsafe { ns_view.superview() };
            reclaim_first_responder_to_host(host.as_deref(), ns_view);
        });
    }

    pub fn release_focus_for_parent(native_parent_view: usize) {
        on_main_thread(move || {
            if native_parent_view == 0 {
                return;
            }
            let parent = native_parent_view as *mut NSView;
            let parent = unsafe { &*parent };
            reclaim_first_responder_to_host(Some(parent), parent);
        });
    }

    pub fn hide(&self) {
        let container = self.container.clone();
        on_main_thread(move || {
            let ns_view: &NSView = &*container;
            ns_view.setHidden(true);
        });
    }
}

fn focus_if_possible(view: &NSView) {
    let Some(window) = view.window() else {
        return;
    };
    if !window.isKeyWindow() {
        return;
    }
    let responder: &NSResponder = view;
    let _: bool = window.makeFirstResponder(Some(responder));
}

fn reapply_webview_layout(container: &WebViewContainer, webview: &WKWebView) {
    let container_view: &NSView = container;
    let frame = container_view.frame();
    let content_bounds = NSRect::new(NSPoint::new(0.0, 0.0), frame.size);
    container_view.setFrame(frame);
    container_view.setBounds(content_bounds);
    container_view.setNeedsLayout(true);
    let webview_view: &NSView = webview;
    webview_view.setFrame(content_bounds);
    webview_view.setBounds(content_bounds);
    webview_view.setNeedsLayout(true);
    let _: () = unsafe { msg_send![&*container_view, layoutSubtreeIfNeeded] };
    let _: () = unsafe { msg_send![&*webview_view, layoutSubtreeIfNeeded] };
    container_view.setNeedsDisplay(true);
    webview_view.setNeedsDisplay(true);
}

fn dispatch_dom_resize_event(webview: &WKWebView) {
    let block =
        StackBlock::new(move |_result: *mut AnyObject, _error: *mut objc2_foundation::NSError| {});
    unsafe {
        webview.evaluateJavaScript_completionHandler(
            &NSString::from_str("window.dispatchEvent(new Event('resize'));"),
            Some(&block),
        );
    }
}

/// If the window's first responder is `subtree_root` or one of its descendants
/// (e.g. the WKWebView inside our container), transfer first responder to
/// `host` — the GPUIView that owns the surface.
///
/// We deliberately avoid `makeFirstResponder:nil`. That call makes the
/// `NSWindow` itself the first responder, which breaks GPUI text input: IME
/// geometry queries (`firstRectForCharacterRange:actualRange:`) on the window
/// return `NSZeroRect`, so the input method candidate window drifts to the
/// bottom-left of the screen. Promoting the GPUIView host keeps AppKit text
/// input routing intact so the active GPUI `InputHandler` (e.g. the browser
/// address field) receives the IME queries.
fn reclaim_first_responder_to_host(host: Option<&NSView>, subtree_root: &NSView) {
    let Some(host) = host else { return };
    let Some(window) = host.window() else {
        return;
    };
    let Some(first_responder) = window.firstResponder() else {
        return;
    };
    let first_responder_ptr = (&*first_responder) as *const NSResponder as *const AnyObject;
    let is_ns_view: bool = unsafe { msg_send![first_responder_ptr, isKindOfClass: class!(NSView)] };
    if !is_ns_view {
        return;
    }
    let first_view = first_responder_ptr.cast::<NSView>();
    let first_view = unsafe { &*first_view };
    if first_view == host {
        return;
    }
    if first_view == subtree_root || first_view.isDescendantOf(subtree_root) {
        let responder: &NSResponder = host;
        let _: bool = window.makeFirstResponder(Some(responder));
    }
}

#[derive(Clone, Debug)]
pub enum NavigationEvent {
    Started { url: String },
    Finished { url: String, title: Option<String> },
    Failed { url: String, error: String },
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

fn on_main_thread_async<F>(f: F)
where
    F: FnOnce() + Send + 'static,
{
    struct AsyncContext {
        f: Option<Box<dyn FnOnce() + Send>>,
    }

    let ctx = Box::new(AsyncContext {
        f: Some(Box::new(f)),
    });
    let ctx_ptr = Box::into_raw(ctx) as *mut std::ffi::c_void;

    unsafe extern "C" fn invoke(ctx_ptr: *mut std::ffi::c_void) {
        let mut ctx = Box::from_raw(ctx_ptr as *mut AsyncContext);
        if let Some(f) = ctx.f.take() {
            f();
        }
    }

    unsafe {
        ah_dispatch_async_f(ah_dispatch_get_main_queue(), ctx_ptr, invoke);
    }
}

// ─── WKWebView Provider ─────────────────────────────────────────────

#[derive(Debug)]
pub struct WKWebViewProvider {
    webview: SendWebView,
    container: SendWebViewContainer,
    /// Hidden offscreen window that hosts the WKWebView so it actually renders.
    /// Without being in a window, WebKit's rendering pipeline won't activate.
    window: SendWindow,
    _ui_delegate: SendWebViewDelegate,
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
        let webview = unsafe {
            let webview: Retained<AgentHouseWebView> = msg_send![
                AgentHouseWebView::alloc(mtm),
                initWithFrame: rect,
                configuration: &*config
            ];
            Retained::into_super(webview)
        };

        // Safari UA for compatibility
        unsafe { webview.setCustomUserAgent(Some(&NSString::from_str(SAFARI_UA))) };
        let ui_delegate = AgentHouseWebViewDelegate::new(mtm);
        unsafe {
            webview.setUIDelegate(Some(ProtocolObject::from_ref(&*ui_delegate)));
        }
        webview.setHidden(true);

        let container = WebViewContainer::new(rect);
        let container_view: &NSView = &*container;
        container_view.setHidden(true);
        container_view.setClipsToBounds(true);
        let webview_view: &NSView = &webview;
        container_view.addSubview(webview_view);
        webview_view.setFrame(NSRect::new(NSPoint::new(0.0, 0.0), rect.size));
        webview_view.setAutoresizingMask(
            NSAutoresizingMaskOptions::ViewWidthSizable
                | NSAutoresizingMaskOptions::ViewHeightSizable,
        );

        // Keep a non-user-facing host window only for lifecycle ownership before
        // the view is attached to the GPUI window. The product path displays the
        // WKWebView inside a direct child container of the GPUI AppKit view.
        let window = unsafe {
            let win: Retained<NSWindow> = msg_send![
                NSWindow::alloc(mtm),
                initWithContentRect: rect,
                styleMask: 0u64,
                backing: 2u64,     // NSBackingStoreBuffered
                defer: false
            ];
            let _: () = msg_send![&win, setReleasedWhenClosed: false];
            // Position far offscreen so the window is invisible to the user
            let offscreen = NSPoint::new(-32000.0, -32000.0);
            let _: () = msg_send![&win, setFrameOrigin: offscreen];
            let _: () = msg_send![&win, orderOut: std::ptr::null::<AnyObject>()];
            win
        };

        Ok(Self {
            webview: SendWebView(Some(webview)),
            container: SendWebViewContainer(Some(container)),
            window: SendWindow(Some(window)),
            _ui_delegate: SendWebViewDelegate(Some(ui_delegate)),
            nav_state: RefCell::new(NavigationState::default()),
            destroyed: false,
        })
    }

    /// Get the underlying NSView for embedding into a GPUI window.
    pub fn ns_view(&self) -> &NSView {
        &*self.webview
    }

    /// Get the hidden host window (for advanced embedding scenarios).
    pub fn host_window(&self) -> &NSWindow {
        &*self.window
    }

    pub fn surface(&self) -> WKWebViewSurface {
        WKWebViewSurface {
            webview: self.webview.clone(),
            container: self.container.clone(),
        }
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
            let webview = ctx.webview.take().unwrap();
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

    fn resize(&mut self, width: u32, height: u32) -> Result<(), WebError> {
        if self.destroyed {
            return Err(WebError::Backend("destroyed".into()));
        }
        let webview = self.webview.clone();
        on_main_thread(move || {
            let webview_view: &NSView = &*webview;
            let content_bounds = NSRect::new(
                NSPoint::new(0.0, 0.0),
                NSSize::new(width.max(1) as f64, height.max(1) as f64),
            );
            webview_view.setFrame(content_bounds);
            webview_view.setBounds(content_bounds);
            webview_view.setNeedsLayout(true);
            let _: () = unsafe { msg_send![&*webview_view, layoutSubtreeIfNeeded] };
            Ok(())
        })
    }

    fn set_visible(&mut self, visible: bool) {
        if self.destroyed {
            return;
        }
        let container = self.container.clone();
        on_main_thread(move || {
            let ns_view: &NSView = &*container;
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
                let webview = self.webview.clone();
                let container = self.container.clone();
                on_main_thread_async(move || {
                    reapply_webview_layout(&container, &webview);
                    dispatch_dom_resize_event(&webview);
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
        let container = self.container.clone();
        let window = self.window.clone();
        on_main_thread(move || {
            let ns_view: &NSView = &*container;
            ns_view.removeFromSuperview();
            let _: () = unsafe { msg_send![&*window, close] };
        });
    }
}

impl Drop for WKWebViewProvider {
    fn drop(&mut self) {
        self.destroy();
    }
}

unsafe impl Send for WKWebViewProvider {}
