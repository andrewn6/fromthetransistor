// talking to the python toolchain that index.html loads into pyodide.
// the page exposes window.ftt = { ready, status, version, files } and
// window.ftt_build(request_json) -> response_json.

use crate::snapshot::{PyError, Snapshot};
use serde::Deserialize;

// one boot rom as the bridge built it: an image, or the stage that failed
#[derive(Clone, Debug, Deserialize)]
pub struct RomBuild {
    #[serde(default, deserialize_with = "opt_hex")]
    pub flat: Option<Vec<u8>>,
    #[serde(default)]
    pub base: u32,
    #[serde(default)]
    pub entry: u32,
    pub error: Option<PyError>,
}

fn opt_hex<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<Vec<u8>>, D::Error> {
    crate::snapshot::hex(d).map(Some)
}

// (repo path, build), in the order the board prefers them
pub type Roms = Vec<(String, RomBuild)>;

fn parse_roms(json: &str) -> Roms {
    serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(json)
        .map(|m| m.into_iter().filter_map(|(k, v)| Some((k, serde_json::from_value(v).ok()?))).collect())
        .unwrap_or_default()
}

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

    pub fn roms(base: u32) -> Roms {
        let f = get(&window(), "ftt_roms");
        let Some(f) = f.dyn_ref::<js_sys::Function>() else { return Vec::new() };
        match f.call1(&JsValue::NULL, &JsValue::from_f64(base as f64)) {
            Ok(v) => parse_roms(&v.as_string().unwrap_or_default()),
            Err(_) => Vec::new(),
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
    pub fn roms(_base: u32) -> Roms {
        Vec::new()
    }
}

pub use imp::{build, reload, roms, state};
