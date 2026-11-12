// web entry point: mounts the app on the #playground canvas (trunk build)
#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

mod app;
mod examples;
mod highlight;
mod theme;
mod views;
mod widgets;

// this is a web app: the native build only exists so `cargo check` works
#[cfg(not(target_arch = "wasm32"))]
fn main() {
    eprintln!("the playground runs in the browser: cd playground/app && trunk serve --open");
}

#[cfg(target_arch = "wasm32")]
fn main() {
    use eframe::wasm_bindgen::JsCast as _;
    eframe::WebLogger::init(log::LevelFilter::Warn).ok();

    wasm_bindgen_futures::spawn_local(async {
        let document = web_sys::window().expect("no window").document().expect("no document");
        let canvas = document
            .get_element_by_id("playground")
            .expect("no #playground canvas")
            .dyn_into::<web_sys::HtmlCanvasElement>()
            .expect("#playground is not a canvas");
        let result = eframe::WebRunner::new()
            .start(canvas, eframe::WebOptions::default(), Box::new(|cc| Ok(Box::new(app::App::new(cc)))))
            .await;
        if let Some(loading) = document.get_element_by_id("loading") {
            match result {
                Ok(_) => loading.remove(),
                Err(e) => loading.set_inner_html(&format!("<p>the playground failed to start: {:?}</p>", e)),
            }
        }
    });
}
