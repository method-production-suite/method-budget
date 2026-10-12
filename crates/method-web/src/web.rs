//! The wasm-bindgen exports (see the crate's notes for the JS API).

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;

use js_sys::{Array, Function, Object, Reflect, Uint8Array};
use method_session::clock::{self, Clock};
use method_session::host::{Error, COMMANDS};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

/// Every open budget, and the commands on them, as JS sees it.
#[wasm_bindgen(js_name = Host)]
pub struct WebHost {
    host: method_session::Host,
}

impl Default for WebHost {
    fn default() -> Self {
        WebHost::new()
    }
}

#[wasm_bindgen(js_class = Host)]
impl WebHost {
    /// One untitled starter budget. The first one installs the browser's
    /// clock.
    #[wasm_bindgen(constructor)]
    pub fn new() -> WebHost {
        install_clock();
        WebHost {
            host: method_session::Host::new(),
        }
    }

    /// Runs `command` with `args_json` (the arguments object as the page's
    /// `invoke` sends it, as JSON; empty means none) and gives back its
    /// reply as JSON. Throws an `Error` with the page's message.
    pub fn dispatch(&self, command: &str, args_json: &str) -> Result<String, JsError> {
        let args = if args_json.trim().is_empty() {
            serde_json::Value::Object(Default::default())
        } else {
            serde_json::from_str(args_json).map_err(|e| {
                JsError::new(
                    &Error::BadArguments {
                        command: command.into(),
                        message: e.to_string(),
                    }
                    .message(),
                )
            })?
        };
        let reply = self.host.dispatch(command, args).map_err(error)?;
        Ok(reply.to_string())
    }

    /// Opens `.mthd` text as a budget named `name`, in a new tab in front.
    /// Its view, as JSON.
    #[wasm_bindgen(js_name = openMthd)]
    pub fn open_mthd(&self, name: &str, text: &str) -> Result<String, JsError> {
        let view = self.host.open_mthd(name, text).map_err(error)?;
        serde_json::to_string(&view).map_err(|e| JsError::new(&e.to_string()))
    }

    /// Budget `doc` (missing: the one in front) as `.mthd` text, saved
    /// under `name` (missing: its own name, as a `.mthd` file name):
    /// `{name, text}`, and no unsaved changes after.
    #[wasm_bindgen(js_name = saveMthd)]
    pub fn save_mthd(&self, doc: Option<f64>, name: Option<String>) -> Result<Object, JsError> {
        let doc = doc.map(|d| d as u64);
        let name = match name {
            Some(n) => crate::mthd_name(&n),
            None => {
                let s = self
                    .host
                    .state()
                    .lock_doc(doc)
                    .map_err(|e| JsError::new(&e))?;
                crate::mthd_name(&s.display_name())
            }
        };
        let saved = self.host.save_mthd(doc, Some(&name)).map_err(error)?;
        let out = Object::new();
        set(&out, "name", &JsValue::from_str(&name))?;
        set(&out, "text", &JsValue::from_str(&saved.text))?;
        Ok(out)
    }

    /// Shows every budget read-only (a viewer), or lets them be changed.
    #[wasm_bindgen(js_name = setReadOnly)]
    pub fn set_read_only(&self, on: bool) {
        self.host.set_read_only(on);
    }

    /// `{kind: "web", commands_version, read_only}`, as JSON.
    #[wasm_bindgen(js_name = hostInfo)]
    pub fn host_info(&self) -> String {
        serde_json::to_string(&self.host.info()).unwrap_or_default()
    }
}

/// Every command `dispatch` runs.
#[wasm_bindgen]
pub fn commands() -> Array {
    COMMANDS.iter().map(|c| JsValue::from_str(c)).collect()
}

fn error(e: Error) -> JsError {
    JsError::new(&e.message())
}

fn set(o: &Object, key: &str, v: &JsValue) -> Result<(), JsError> {
    Reflect::set(o, &JsValue::from_str(key), v)
        .map(|_| ())
        .map_err(|_| JsError::new("Couldn't build the reply."))
}

// ---------------------------------------------------------------------------
// The clock

/// The browser's clock: `Date.now()` for the wall and, held so it never
/// goes back, for the steady clock; unique names from a random seed and a
/// counter.
struct BrowserClock {
    seed: u64,
    last: AtomicU64,
    count: AtomicU64,
}

impl Clock for BrowserClock {
    fn wall_ms(&self) -> u64 {
        now_ms()
    }

    fn mono_ms(&self) -> u64 {
        let now = now_ms();
        self.last.fetch_max(now, Ordering::Relaxed).max(now)
    }

    fn unique(&self) -> String {
        let n = self.count.fetch_add(1, Ordering::Relaxed);
        format!("{:x}-{:x}-{n:x}", self.seed, now_ms())
    }
}

fn now_ms() -> u64 {
    let ms = js_sys::Date::now();
    if ms.is_finite() && ms > 0.0 {
        ms as u64
    } else {
        0
    }
}

/// Installs [`BrowserClock`] once, before the first budget.
fn install_clock() {
    static ONCE: OnceLock<()> = OnceLock::new();
    ONCE.get_or_init(|| {
        clock::install(Box::new(BrowserClock {
            seed: random_seed(),
            last: AtomicU64::new(0),
            count: AtomicU64::new(0),
        }));
    });
}

/// Eight bytes from `crypto.getRandomValues`, or the time when the page has
/// no `crypto` (never in a current browser).
fn random_seed() -> u64 {
    let from_crypto = || -> Option<u64> {
        let crypto = Reflect::get(&js_sys::global(), &JsValue::from_str("crypto")).ok()?;
        let get: Function = Reflect::get(&crypto, &JsValue::from_str("getRandomValues"))
            .ok()?
            .dyn_into()
            .ok()?;
        let bytes = Uint8Array::new_with_length(8);
        get.call1(&crypto, &bytes).ok()?;
        let mut b = [0u8; 8];
        bytes.copy_to(&mut b);
        Some(u64::from_le_bytes(b))
    };
    from_crypto().unwrap_or_else(now_ms)
}
