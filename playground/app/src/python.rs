// talking to the python toolchain that index.html loads into pyodide.
// the page exposes window.ftt = { ready, status, version, files } and
// window.ftt_build(request_json) -> response_json.

use crate::snapshot::Snapshot;

#[derive(Clone, Debug, Default)]
pub struct State {
    pub ready: bool,
    pub status: String,
    // bumps every time the .py files are (re)loaded
    pub version: u32,
    pub files: Vec<String>,
}

#[cfg(target_arch = "wasm32")]
mod imp {
    use super::*;
    use eframe::wasm_bindgen::{JsCast, JsValue};

    fn get(o: &JsValue, k: &str) -> JsValue {
        js_sys::Reflect::get(o, &JsValue::from_str(k)).unwrap_or(JsValue::UNDEFINED)
    }

    fn window() -> JsValue {
        web_sys::window().map(JsValue::from).unwrap_or(JsValue::UNDEFINED)
    }

    pub fn state() -> State {
        let ftt = get(&window(), "ftt");
        if ftt.is_undefined() {
            return State { status: "page has no python loader".into(), ..Default::default() };
        }
        let files = get(&ftt, "files");
        let files = js_sys::Array::from(&files).iter().filter_map(|v| v.as_string()).collect();
        State {
            ready: get(&ftt, "ready").as_bool().unwrap_or(false),
            status: get(&ftt, "status").as_string().unwrap_or_default(),
            version: get(&ftt, "version").as_f64().unwrap_or(0.0) as u32,
            files,
        }
    }

    pub fn build(request: &str) -> Snapshot {
        let f = get(&window(), "ftt_build");
        let Some(f) = f.dyn_ref::<js_sys::Function>() else {
            return Snapshot::failed("ftt_build is not defined");
        };
        match f.call1(&JsValue::NULL, &JsValue::from_str(request)) {
            Ok(v) => match v.as_string() {
                Some(s) => serde_json::from_str(&s).unwrap_or_else(|e| Snapshot::failed(format!("bad bridge response: {}", e))),
                None => Snapshot::failed("bridge returned a non-string"),
            },
            Err(e) => Snapshot::failed(format!("python crashed: {:?}", e)),
        }
    }

    pub fn reload() {
        let f = get(&window(), "ftt_reload");
        if let Some(f) = f.dyn_ref::<js_sys::Function>() {
            let _ = f.call0(&JsValue::NULL);
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod imp {
    use super::*;
    pub fn state() -> State {
        State { status: "python only runs in the browser build".into(), ..Default::default() }
    }
    pub fn build(_request: &str) -> Snapshot {
        Snapshot::failed("python only runs in the browser build")
    }
    pub fn reload() {}
}

pub use imp::{build, reload, state};
