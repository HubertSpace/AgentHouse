//! Linux WebView backend for AgentHouse browser integration.
//!
//! Uses WebKitGTK via a channel-based dispatch pattern:
//! - `LinuxWebViewBackend` holds no GTK objects (only a channel sender).
//! - A dedicated GTK worker thread owns the `gtk::OffscreenWindow` + `webkit2gtk::WebView`.
//! - Each `BrowserBackend` method sends a command through the channel and awaits the result.

use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use ah_core::Timestamp;
use ah_web::{
    BrowserAction, BrowserBackend, BrowserBackendSnapshot, BrowserEngine,
    BrowserFrameSnapshot, BrowserInput, PageSnapshot, ViewportSize, WebError,
};

use gtk::prelude::*;
use webkit2gtk::WebViewExt;

// ---------------------------------------------------------------------------
// Navigation state — shared via Arc<Mutex> between backend and GTK worker
// ---------------------------------------------------------------------------

#[derive(Debug, Default)]
struct NavState {
    current_url: String,
    title: Option<String>,
    is_loading: bool,
    is_loaded: bool,
}

// ---------------------------------------------------------------------------
// Commands sent from LinuxWebViewBackend → GTK worker thread
// ---------------------------------------------------------------------------

enum BackendCmd {
    Navigate {
        url: String,
        result_tx: Sender<Result<(), WebError>>,
    },
    Reload {
        result_tx: Sender<Result<(), WebError>>,
    },
    GoBack {
        result_tx: Sender<Result<(), WebError>>,
    },
    GoForward {
        result_tx: Sender<Result<(), WebError>>,
    },
    Resize {
        size: ViewportSize,
        result_tx: Sender<Result<(), WebError>>,
    },
    EvaluateJs {
        expression: String,
        result_tx: Sender<Result<Option<String>, WebError>>,
    },
    TakeScreenshot {
        result_tx: Sender<Result<Option<BrowserFrameSnapshot>, WebError>>,
    },
    Destroy {
        result_tx: Sender<()>,
    },
}

// ---------------------------------------------------------------------------
// Public backend
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct LinuxWebViewBackend {
    cmd_tx: Sender<BackendCmd>,
    nav_state: Arc<Mutex<NavState>>,
    viewport: Mutex<ViewportSize>,
}

impl LinuxWebViewBackend {
    pub fn new() -> Result<Self, String> {
        let (cmd_tx, cmd_rx) = std::sync::mpsc::channel();
        let nav_state = Arc::new(Mutex::new(NavState::default()));
        let nav_state_clone = nav_state.clone();

        thread::Builder::new()
            .name("ah-gtk-worker".into())
            .spawn(move || gtk_worker_main(cmd_rx, nav_state_clone))
            .map_err(|e| format!("failed to spawn GTK worker thread: {e}"))?;

        Ok(Self {
            cmd_tx,
            nav_state,
            viewport: Mutex::new(ViewportSize {
                width: 1280,
                height: 720,
            }),
        })
    }

    fn send_cmd(&self, cmd: BackendCmd) -> Result<(), WebError> {
        self.cmd_tx
            .send(cmd)
            .map_err(|e| WebError::Backend(e.to_string()))
    }

    fn eval_js(&self, expression: &str) -> Result<Option<String>, WebError> {
        let (result_tx, result_rx) = std::sync::mpsc::channel();
        self.send_cmd(BackendCmd::EvaluateJs {
            expression: expression.to_string(),
            result_tx,
        })?;
        result_rx
            .recv()
            .map_err(|e| WebError::Backend(e.to_string()))?
    }
}

// ---------------------------------------------------------------------------
// BrowserBackend implementation
// ---------------------------------------------------------------------------

impl BrowserBackend for LinuxWebViewBackend {
    fn engine(&self) -> BrowserEngine {
        BrowserEngine::Native
    }

    fn open(&mut self, url: &str) -> Result<(), WebError> {
        self.navigate(url)
    }

    fn navigate(&mut self, url: &str) -> Result<(), WebError> {
        let (result_tx, result_rx) = std::sync::mpsc::channel();
        self.send_cmd(BackendCmd::Navigate {
            url: url.to_string(),
            result_tx,
        })?;
        result_rx
            .recv()
            .map_err(|e| WebError::Backend(e.to_string()))?
    }

    fn reload(&mut self) -> Result<(), WebError> {
        let (result_tx, result_rx) = std::sync::mpsc::channel();
        self.send_cmd(BackendCmd::Reload { result_tx })?;
        result_rx
            .recv()
            .map_err(|e| WebError::Backend(e.to_string()))?
    }

    fn go_back(&mut self) -> Result<(), WebError> {
        let (result_tx, result_rx) = std::sync::mpsc::channel();
        self.send_cmd(BackendCmd::GoBack { result_tx })?;
        result_rx
            .recv()
            .map_err(|e| WebError::Backend(e.to_string()))?
    }

    fn go_forward(&mut self) -> Result<(), WebError> {
        let (result_tx, result_rx) = std::sync::mpsc::channel();
        self.send_cmd(BackendCmd::GoForward { result_tx })?;
        result_rx
            .recv()
            .map_err(|e| WebError::Backend(e.to_string()))?
    }

    fn resize(&mut self, size: ViewportSize) -> Result<(), WebError> {
        *self
            .viewport
            .lock()
            .map_err(|e| WebError::Backend(e.to_string()))? = size;
        let (result_tx, result_rx) = std::sync::mpsc::channel();
        self.send_cmd(BackendCmd::Resize { size, result_tx })?;
        result_rx
            .recv()
            .map_err(|e| WebError::Backend(e.to_string()))?
    }

    fn input(&mut self, input: BrowserInput) -> Result<(), WebError> {
        match input {
            BrowserInput::Back => self.go_back(),
            BrowserInput::Forward => self.go_forward(),
            BrowserInput::Reload => self.reload(),
            input => Err(WebError::Unsupported(format!(
                "linux webview does not support raw input: {input:?}"
            ))),
        }
    }

    fn action(&mut self, action: &BrowserAction) -> Result<Option<String>, WebError> {
        match action {
            BrowserAction::Navigate { url } => {
                self.navigate(url)?;
                Ok(None)
            }
            BrowserAction::Reload => {
                self.reload()?;
                Ok(None)
            }
            BrowserAction::Back => {
                self.go_back()?;
                Ok(None)
            }
            BrowserAction::Forward => {
                self.go_forward()?;
                Ok(None)
            }
            BrowserAction::Snapshot => Ok(None),
            BrowserAction::Click { selector } => {
                let js = format!(
                    "(function(){{\
                       var el = document.querySelector('{sel}');\
                       if(el){{ el.click(); return 'clicked'; }}\
                       return 'not found';\
                     }})()",
                    sel = js_escape(selector)
                );
                self.eval_js(&js)
            }
            BrowserAction::Fill { selector, value } => {
                let js = format!(
                    "(function(){{\
                       var el = document.querySelector('{sel}');\
                       if(el){{\
                         el.value = '{val}';\
                         el.dispatchEvent(new Event('input',{{bubbles:true}}));\
                         el.dispatchEvent(new Event('change',{{bubbles:true}}));\
                         return 'filled';\
                       }}\
                       return 'not found';\
                     }})()",
                    sel = js_escape(selector),
                    val = js_escape(value),
                );
                self.eval_js(&js)
            }
            BrowserAction::Type { selector, text } => {
                let js = format!(
                    "(function(){{\
                       var el = document.querySelector('{sel}');\
                       if(el){{\
                         el.focus();\
                         el.value += '{txt}';\
                         el.dispatchEvent(new Event('input',{{bubbles:true}}));\
                         return 'typed';\
                       }}\
                       return 'not found';\
                     }})()",
                    sel = js_escape(selector),
                    txt = js_escape(text),
                );
                self.eval_js(&js)
            }
            BrowserAction::PressKey { key, .. } => {
                let js = format!(
                    "(function(){{\
                       document.dispatchEvent(new KeyboardEvent('keydown',{{key:'{k}',bubbles:true}}));\
                       document.dispatchEvent(new KeyboardEvent('keypress',{{key:'{k}',bubbles:true}}));\
                       document.dispatchEvent(new KeyboardEvent('keyup',{{key:'{k}',bubbles:true}}));\
                       return 'pressed';\
                     }})()",
                    k = js_escape(key),
                );
                self.eval_js(&js)
            }
            BrowserAction::SelectOption { selector, value } => {
                let js = format!(
                    "(function(){{\
                       var el = document.querySelector('{sel}');\
                       if(el){{\
                         el.value = '{val}';\
                         el.dispatchEvent(new Event('change',{{bubbles:true}}));\
                         return 'selected';\
                       }}\
                       return 'not found';\
                     }})()",
                    sel = js_escape(selector),
                    val = js_escape(value),
                );
                self.eval_js(&js)
            }
            BrowserAction::Evaluate { expression } => self.eval_js(expression),
        }
    }

    fn snapshot(&mut self) -> Result<BrowserBackendSnapshot, WebError> {
        let nav = self
            .nav_state
            .lock()
            .map_err(|e| WebError::Backend(e.to_string()))?;
        let url = if nav.current_url.is_empty() {
            "about:blank".to_string()
        } else {
            nav.current_url.clone()
        };
        let title = nav.title.clone();
        let is_loaded = nav.is_loaded;
        drop(nav);

        // Extract page text via JS
        let text = if is_loaded && url != "about:blank" {
            self.eval_js("document.body ? document.body.innerText : ''")
                .ok()
                .flatten()
        } else {
            None
        };

        // Take screenshot
        let frame: Option<BrowserFrameSnapshot> = if is_loaded && url != "about:blank" {
            let (result_tx, result_rx) = std::sync::mpsc::channel();
            self.send_cmd(BackendCmd::TakeScreenshot { result_tx })?;
            match result_rx.recv() {
                Ok(Ok(Some(f))) => Some(f),
                Ok(Ok(None)) => None,
                Ok(Err(e)) => return Err(e),
                Err(e) => return Err(WebError::Backend(e.to_string())),
            }
        } else {
            None
        };

        let page = PageSnapshot {
            url,
            title,
            text,
            status: None,
            byte_count: None,
            truncated: false,
            captured_at: Timestamp::now(),
        };

        Ok(BrowserBackendSnapshot {
            page: Some(page),
            frame,
        })
    }
}

impl Drop for LinuxWebViewBackend {
    fn drop(&mut self) {
        let (tx, rx) = std::sync::mpsc::channel();
        let _ = self.cmd_tx.send(BackendCmd::Destroy { result_tx: tx });
        let _ = rx.recv_timeout(Duration::from_secs(5));
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn js_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('\'', "\\'")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}

// ---------------------------------------------------------------------------
// GTK worker thread
// ---------------------------------------------------------------------------

fn gtk_worker_main(cmd_rx: Receiver<BackendCmd>, nav_state: Arc<Mutex<NavState>>) {
    if gtk::init().is_err() {
        tracing::error!("ah-webview-linux: failed to initialize GTK");
        return;
    }

    let context = glib::MainContext::default();
    let _guard = context
        .acquire()
        .expect("ah-webview-linux: failed to acquire main context");

    // Create offscreen window (renders to a cairo surface without displaying)
    let window = gtk::OffscreenWindow::new();
    window.set_default_size(1280, 720);

    let webview = webkit2gtk::WebView::new();
    window.add(&webview);
    window.show_all();

    // -- Connect navigation signals --

    let ns = nav_state.clone();
    webview.connect_load_changed(move |wv, load_event| {
        let mut state = ns.lock().unwrap();
        match load_event {
            webkit2gtk::LoadEvent::Started => {
                state.is_loading = true;
                state.is_loaded = false;
            }
            webkit2gtk::LoadEvent::Finished => {
                state.is_loading = false;
                state.is_loaded = true;
                state.current_url = wv.uri().map(|u| u.to_string()).unwrap_or_default();
                state.title = wv.title().map(|t| t.to_string());
            }
            _ => {}
        }
    });

    let ns = nav_state.clone();
    webview.connect_load_failed(move |_wv, _load_event, failing_uri, _error| {
        let mut state = ns.lock().unwrap();
        state.is_loading = false;
        state.is_loaded = false;
        state.current_url = failing_uri.to_string();
        false // do not propagate
    });

    // -- Main loop with command polling --

    let main_loop = glib::MainLoop::new(Some(&context), false);
    let main_loop_clone = main_loop.clone();
    let win = window.clone();
    let wv = webview.clone();

    glib::idle_add_local(move || {
        while let Ok(cmd) = cmd_rx.try_recv() {
            match cmd {
                BackendCmd::Navigate { url, result_tx } => {
                    wv.load_uri(&url);
                    let _ = result_tx.send(Ok(()));
                }
                BackendCmd::Reload { result_tx } => {
                    wv.reload();
                    let _ = result_tx.send(Ok(()));
                }
                BackendCmd::GoBack { result_tx } => {
                    if wv.can_go_back() {
                        wv.go_back();
                        let _ = result_tx.send(Ok(()));
                    } else {
                        let _ = result_tx.send(Err(WebError::Unsupported(
                            "no back history".into(),
                        )));
                    }
                }
                BackendCmd::GoForward { result_tx } => {
                    if wv.can_go_forward() {
                        wv.go_forward();
                        let _ = result_tx.send(Ok(()));
                    } else {
                        let _ = result_tx.send(Err(WebError::Unsupported(
                            "no forward history".into(),
                        )));
                    }
                }
                BackendCmd::Resize { size, result_tx } => {
                    win.set_default_size(size.width as i32, size.height as i32);
                    let _ = result_tx.send(Ok(()));
                }
                BackendCmd::EvaluateJs {
                    expression,
                    result_tx,
                } => {
                    gtk_eval_js(&wv, &expression, result_tx);
                }
                BackendCmd::TakeScreenshot { result_tx } => {
                    let result = gtk_take_screenshot(&win);
                    let _ = result_tx.send(result);
                }
                BackendCmd::Destroy { result_tx } => {
                    // hide instead of destroy (destroy is unsafe in gtk-rs 0.18)
                    win.hide();
                    let _ = result_tx.send(());
                    main_loop_clone.quit();
                    return glib::ControlFlow::Break;
                }
            }
        }
        glib::ControlFlow::Continue
    });

    main_loop.run();
    tracing::info!("ah-webview-linux: GTK worker thread exiting");
}

// ---------------------------------------------------------------------------
// GTK worker helpers (run on the GTK thread)
// ---------------------------------------------------------------------------

fn gtk_eval_js(
    webview: &webkit2gtk::WebView,
    expression: &str,
    result_tx: Sender<Result<Option<String>, WebError>>,
) {
    let expr = expression.to_string();
    webview.run_javascript(&expr, gio::Cancellable::NONE, move |result| {
        let value = match result {
            Ok(js_result) => {
                match js_result.js_value() {
                    Some(jv) => {
                        let s = jv.to_string();
                        if s.is_empty() || s == "undefined" || s == "null" {
                            Ok(None)
                        } else {
                            Ok(Some(s))
                        }
                    }
                    None => Ok(None),
                }
            }
            Err(e) => Err(WebError::Backend(format!("JS eval error: {e}"))),
        };
        let _ = result_tx.send(value);
    });
}

fn gtk_take_screenshot(
    window: &gtk::OffscreenWindow,
) -> Result<Option<BrowserFrameSnapshot>, WebError> {
    // TODO: OffscreenWindow::surface() returns a cairo::Surface but
    // cairo-rs 0.18 only exposes write_to_png on ImageSurface.
    // For now, skip screenshots. A future iteration can use
    // gdk::pixbuf_get_from_surface() or webkit2gtk::WebView::snapshot().
    let _surface = window.surface();
    Ok(None)
}
