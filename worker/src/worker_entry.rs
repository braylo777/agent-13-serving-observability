//! Cloudflare Worker entry (wasm32 only). Same routes and JSON as the old src/index.js.
use crate::api::{handle, Host};
use crate::obs::Kv;
use crate::page::PAGE;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use wasm_bindgen::JsCast;
use worker::*;

struct Cf {
    env: Env,
    ip: String,
}

impl Kv for Cf {
    async fn get(&self, key: &str) -> Option<String> {
        self.env.kv("TRACES").ok()?.get(key).text().await.ok().flatten()
    }
    async fn put(&self, key: &str, val: &str) {
        if let Ok(k) = self.env.kv("TRACES") {
            if let Ok(b) = k.put(key, val) {
                let _ = b.execute().await;
            }
        }
    }
    async fn delete(&self, key: &str) {
        if let Ok(k) = self.env.kv("TRACES") {
            let _ = k.delete(key).await;
        }
    }
}

impl Host for Cf {
    fn has_kv(&self) -> bool {
        self.env.kv("TRACES").is_ok()
    }
    async fn ai(&self, model: &str, input: Value) -> std::result::Result<Value, String> {
        self.env.ai("AI").map_err(|e| e.to_string())?.run::<Value, Value>(model, input).await.map_err(|e| e.to_string())
    }
    async fn limited(&self) -> bool {
        let rl = match js_sys::Reflect::get(self.env.as_ref(), &"RATE_LIMITER".into()) {
            Ok(v) if !v.is_undefined() && !v.is_null() => v,
            _ => return false,
        };
        let opts = js_sys::Object::new();
        let _ = js_sys::Reflect::set(&opts, &"key".into(), &self.ip.clone().into());
        let Ok(f) = js_sys::Reflect::get(&rl, &"limit".into()).and_then(|f| f.dyn_into::<js_sys::Function>().map_err(|e| e)) else { return false };
        let Ok(p) = f.call1(&rl, &opts) else { return false };
        match wasm_bindgen_futures::JsFuture::from(js_sys::Promise::from(p)).await {
            Ok(r) => !js_sys::Reflect::get(&r, &"success".into()).ok().and_then(|v| v.as_bool()).unwrap_or(true),
            Err(_) => false,
        }
    }
    fn now(&self) -> f64 {
        let g = js_sys::global();
        js_sys::Reflect::get(&g, &"performance".into()).ok().and_then(|p| js_sys::Reflect::get(&p, &"now".into()).ok().and_then(|f| f.dyn_into::<js_sys::Function>().ok().and_then(|f| f.call0(&p).ok()))).and_then(|v| v.as_f64()).unwrap_or_else(js_sys::Date::now)
    }
    fn epoch(&self) -> i64 {
        js_sys::Date::now() as i64
    }
    fn trace_id(&self) -> String {
        let g = js_sys::global();
        let arr = js_sys::Uint8Array::new_with_length(16);
        if let Ok(c) = js_sys::Reflect::get(&g, &"crypto".into()) {
            if let Ok(f) = js_sys::Reflect::get(&c, &"getRandomValues".into()).and_then(|f| f.dyn_into::<js_sys::Function>().map_err(|e| e)) {
                let _ = f.call1(&c, &arr);
            }
        }
        arr.to_vec().iter().map(|b| format!("{:02x}", b)).collect()
    }
    fn sha_prefix(&self, q: &str) -> String {
        Sha256::digest(q.as_bytes())[..4].iter().map(|b| format!("{:x}", b)).collect()
    }
}

#[event(fetch)]
async fn fetch(mut req: Request, env: Env, _ctx: Context) -> Result<Response> {
    let url = req.url()?;
    let (path, method) = (url.path().to_string(), req.method());
    let ip = req.headers().get("cf-connecting-ip")?.unwrap_or_else(|| "anon".into());
    let body: Value = if method == Method::Post { req.json().await.unwrap_or(json!({})) } else { json!({}) };
    let host = Cf { env, ip };
    let (status, v) = handle(&host, method.as_ref(), &path, url.query().unwrap_or(""), &body).await;
    if status == 0 {
        let h = Headers::new();
        h.set("content-type", "text/html;charset=utf-8")?;
        return Ok(Response::ok(PAGE)?.with_headers(h));
    }
    Ok(Response::from_json(&v)?.with_status(status))
}
