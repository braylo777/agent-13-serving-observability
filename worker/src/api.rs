//! Routes. `Host` supplies the platform pieces (Workers AI, TRACES KV, rate limiter, clocks, ids) so the logic tests natively.
use crate::obs::*;
use regex::Regex;
use serde_json::{json, Value};

#[allow(async_fn_in_trait)]
pub trait Host: Kv {
    fn has_kv(&self) -> bool;
    async fn ai(&self, model: &str, input: Value) -> Result<Value, String>;
    async fn limited(&self) -> bool;
    fn now(&self) -> f64; // monotonic ms (performance.now)
    fn epoch(&self) -> i64; // Date.now
    fn trace_id(&self) -> String;
    fn sha_prefix(&self, q: &str) -> String; // first 4 bytes of SHA-256, each byte as unpadded hex (JS toString(16))
}

pub fn cap(s: Option<&str>, n: usize) -> bool {
    matches!(s, Some(s) if !s.trim().is_empty() && s.chars().count() <= n)
}
fn text(r: &Value) -> String {
    match r.get("response") {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Null) | None => r.to_string(),
        Some(v) => v.to_string(),
    }
}
fn query(q: &str, key: &str) -> Option<String> {
    Regex::new(&format!(r"(?:^|&){}=([^&]*)", regex::escape(key))).unwrap().captures(q).map(|c| c[1].replace('+', " "))
}
fn slice_end(n_raw: Option<String>, len: usize) -> usize {
    let k = match n_raw.as_deref() {
        None | Some("") => 25.0,
        Some(s) => s.trim().parse::<f64>().unwrap_or(f64::NAN),
    };
    if k.is_nan() { return 0; }
    let k = k.trunc() as i64;
    (if k >= 0 { k } else { len as i64 + k }).clamp(0, len as i64) as usize
}

async fn save<H: Host>(h: &H, t: &Value) {
    if h.has_kv() {
        store(h, t).await;
    }
}

/// One traced model call: build prompt -> Workers AI -> parse usage -> cost.
pub async fn serve<H: Host>(h: &H, tr: &mut Tracer<'_>, prompt: &str) -> Result<Value, (String, ())> {
    let (s0, _) = tr.start();
    let messages = json!([{"role":"system","content":"You are a concise assistant. Answer in under 80 words."},{"role":"user","content":prompt}]);
    tr.end("build_prompt", s0, json!({}), None);
    let (s1, _) = tr.start();
    let r = h.ai(MODEL, json!({"messages": messages, "max_tokens": 200})).await;
    match r {
        Err(e) => {
            tr.end("model.generate", s1, json!({"model": MODEL}), Some(e.clone()));
            Err((e, ()))
        }
        Ok(r) => {
            tr.end("model.generate", s1, json!({"model": MODEL}), None);
            let answer = text(&r);
            let measured = r.get("usage").and_then(|u| u.get("prompt_tokens")).and_then(|p| p.as_f64()).is_some();
            let tokens = if measured {
                json!({"in": r["usage"]["prompt_tokens"], "out": r["usage"]["completion_tokens"], "source": "model usage"})
            } else {
                let joined: Vec<&str> = messages.as_array().unwrap().iter().map(|m| m["content"].as_str().unwrap()).collect();
                json!({"in": est_tokens(&joined.join(" ")), "out": est_tokens(&answer), "source": "chars/4 estimate"})
            };
            let cost = cost_estimate(MODEL, tokens["in"].as_f64().unwrap_or(0.0), tokens["out"].as_f64().unwrap_or(0.0));
            Ok(json!({"answer": answer, "tokens": tokens, "cost": cost}))
        }
    }
}

/// Returns (status, json); status 0 = serve the page. `body` is the parsed JSON body (POST).
pub async fn handle<H: Host>(h: &H, method: &str, path: &str, qs: &str, body: &Value) -> (u16, Value) {
    let rec = || async { if h.has_kv() { recent(h).await } else { vec![] } };
    if path == "/api/metrics" {
        let ts = rec().await;
        return (200, json!({"window": ts.len(), "routes": summarize(&ts), "pricing": pricing()}));
    }
    if path == "/api/traces" {
        let ts = rec().await;
        let k = slice_end(query(qs, "n"), ts.len());
        return (200, Value::Array(ts[..k].to_vec()));
    }
    if let Some(id) = path.strip_prefix("/api/traces/") {
        let id = id.rsplit('/').next().unwrap_or("");
        return match rec().await.into_iter().find(|x| x["trace_id"] == id) {
            Some(t) => (200, t),
            None => (404, json!({"error": "not found"})),
        };
    }
    let now = || h.now();
    if path == "/api/ping" {
        let mut tr = Tracer::new("/api/ping", &now, h.trace_id(), h.epoch());
        let (s0, _) = tr.start();
        let v = h.sha_prefix(&query(qs, "q").filter(|s| !s.is_empty()).unwrap_or_else(|| "ping".into()));
        tr.end("work.hash", s0, json!({}), None);
        let t = tr.finish(200, json!({"synthetic": true}));
        save(h, &t).await;
        return (200, json!({"ok": true, "v": v, "trace_id": t["trace_id"]}));
    }
    if method == "POST" && path == "/api/chat" {
        let mut tr = Tracer::new("/api/chat", &now, h.trace_id(), h.epoch());
        let (s0, _) = tr.start();
        let lim = h.limited().await;
        tr.end("rate_limit", s0, json!({}), None);
        if lim {
            let t = tr.finish(429, json!({}));
            save(h, &t).await;
            return (429, json!({"error": "Rate limit: try again in a minute.", "trace_id": t["trace_id"]}));
        }
        let prompt = body.get("prompt").and_then(|v| v.as_str());
        if !cap(prompt, 500) {
            return (400, json!({"error": "prompt required (<=500 chars)"}));
        }
        return match serve(h, &mut tr, prompt.unwrap()).await {
            Ok(r) => {
                let t = tr.finish(200, json!({"tokens": r["tokens"], "cost": r["cost"]}));
                save(h, &t).await;
                (200, json!({"answer": r["answer"], "tokens": r["tokens"], "cost": r["cost"], "trace_id": t["trace_id"], "duration_ms": t["duration_ms"], "spans": t["spans"]}))
            }
            Err((e, _)) => {
                let t = tr.finish(502, json!({}));
                save(h, &t).await;
                (502, json!({"error": format!("Model error: {e}"), "trace_id": t["trace_id"]}))
            }
        };
    }
    (0, Value::Null)
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::executor::block_on;
    use std::cell::{Cell, RefCell};
    use std::collections::HashMap;

    struct H { ai: Box<dyn Fn() -> Result<Value, String>>, kv: RefCell<HashMap<String, String>>, lim: bool, clock: Cell<f64>, n: Cell<u32>, on: bool }
    impl Kv for H {
        async fn get(&self, k: &str) -> Option<String> { self.kv.borrow().get(k).cloned() }
        async fn put(&self, k: &str, v: &str) { self.kv.borrow_mut().insert(k.into(), v.into()); }
        async fn delete(&self, k: &str) { self.kv.borrow_mut().remove(k); }
    }
    impl Host for H {
        fn has_kv(&self) -> bool { self.on }
        async fn ai(&self, _m: &str, _i: Value) -> Result<Value, String> { (self.ai)() }
        async fn limited(&self) -> bool { self.lim }
        fn now(&self) -> f64 { self.clock.set(self.clock.get() + 5.0); self.clock.get() }
        fn epoch(&self) -> i64 { 1_700_000_000_000 }
        fn trace_id(&self) -> String { self.n.set(self.n.get() + 1); format!("{:032x}", self.n.get()) }
        fn sha_prefix(&self, _q: &str) -> String { "abcd".into() }
    }
    fn host(ai: impl Fn() -> Result<Value, String> + 'static, lim: bool) -> H { H { ai: Box::new(ai), kv: Default::default(), lim, clock: Cell::new(0.0), n: Cell::new(0), on: true } }
    fn ok_ai() -> Result<Value, String> { Ok(json!({"response": "hi", "usage": {"prompt_tokens": 40, "completion_tokens": 20}})) }

    #[test]
    fn cost_pct_tokens_summarize() {
        let c = cost_estimate(MODEL, 1e6, 0.0);
        assert_eq!((c["neurons"].as_f64(), c["usd"].as_f64()), (Some(26668.0), Some(0.293348)));
        assert_eq!(cost_estimate(MODEL, 0.0, 1e6)["neurons"].as_f64(), Some(204805.0));
        assert!(cost_estimate("unknown", 1.0, 1.0).is_null());
        assert_eq!(cost_estimate(MODEL, 0.0, 0.0)["usd"].as_f64(), Some(0.0));
        let xs: Vec<f64> = (1..=100).map(|i| i as f64).collect();
        assert_eq!((pct(&xs, 50.0), pct(&xs, 95.0), pct(&[], 50.0), pct(&[7.0], 95.0)), (json!(50.0), json!(95.0), Value::Null, json!(7.0)));
        assert_eq!((pct(&[5.0, 1.0, 3.0, 2.0, 4.0], 50.0), pct(&[5.0, 1.0, 3.0, 2.0, 4.0], 100.0)), (json!(3.0), json!(5.0)));
        assert_eq!((est_tokens("abcdefgh"), est_tokens("")), (2, 0));
        let s = summarize(&[json!({"route":"/a","status":200,"duration_ms":10}), json!({"route":"/a","status":502,"duration_ms":30}), json!({"route":"/b","status":200,"duration_ms":5,"tokens":{"in":10,"out":4},"cost":{"usd":0.001}})]);
        assert_eq!((s["/a"]["n"].as_i64(), s["/a"]["errors"].as_i64(), s["/a"]["p95_ms"].as_f64()), (Some(2), Some(1), Some(30.0)));
        assert_eq!((s["/b"]["avg_tokens_in"].as_f64(), s["/b"]["est_cost_usd_per_req"].as_f64()), (Some(10.0), Some(0.001)));
    }
    #[test]
    fn kv_store_caps_200_newest_first() {
        let h = host(ok_ai, false);
        for i in 0..205 { block_on(store(&h, &json!({"trace_id": format!("id{i}"), "i": i}))); }
        let r = block_on(recent(&h));
        assert_eq!((r.len(), r[0]["i"].as_i64(), r.last().unwrap()["i"].as_i64()), (200, Some(204), Some(5)));
    }
    #[test]
    fn http_chat_paths() {
        let h = host(ok_ai, false);
        let (s, j) = block_on(handle(&h, "POST", "/api/chat", "", &json!({"prompt": "hello"})));
        assert_eq!((s, j["tokens"]["in"].as_i64(), j["tokens"]["source"].as_str()), (200, Some(40), Some("model usage")));
        assert!(j["cost"]["usd"].as_f64().unwrap() > 0.0);
        let names: Vec<&str> = j["spans"].as_array().unwrap().iter().map(|x| x["name"].as_str().unwrap()).collect();
        assert_eq!(names, ["rate_limit", "build_prompt", "model.generate"]);
        let id = j["trace_id"].as_str().unwrap();
        assert_eq!(block_on(handle(&h, "GET", &format!("/api/traces/{id}"), "", &json!({}))).1["trace_id"], id);
        let h = host(|| Ok(json!({"response": "abcd"})), false);
        let j = block_on(handle(&h, "POST", "/api/chat", "", &json!({"prompt": "hi"}))).1;
        assert_eq!((j["tokens"]["source"].as_str(), j["tokens"]["out"].as_i64()), (Some("chars/4 estimate"), Some(1)));
        let h = host(|| Err("quota".into()), false);
        assert_eq!(block_on(handle(&h, "POST", "/api/chat", "", &json!({"prompt": "hi"}))).0, 502);
        assert_eq!(block_on(handle(&h, "GET", "/api/metrics", "", &json!({}))).1["routes"]["/api/chat"]["errors"], 1);
        assert_eq!(block_on(recent(&h))[0]["spans"].as_array().unwrap().last().unwrap()["error"], "quota");
        assert_eq!(block_on(handle(&host(ok_ai, false), "POST", "/api/chat", "", &json!({}))).0, 400);
        let l = host(ok_ai, true);
        assert_eq!(block_on(handle(&l, "POST", "/api/chat", "", &json!({"prompt": "x"}))).0, 429);
        assert_eq!(block_on(handle(&l, "GET", "/api/ping", "", &json!({}))).1["ok"], true);
        assert_eq!(block_on(handle(&l, "GET", "/api/traces/nope", "", &json!({}))).0, 404);
        assert_eq!(block_on(handle(&l, "GET", "/", "", &json!({}))).0, 0);
    }
    #[test]
    fn trace_ids_unique_hex() {
        let h = host(ok_ai, false);
        let ids: std::collections::HashSet<String> = (0..1000).map(|_| h.trace_id()).collect();
        assert!(ids.len() == 1000 && ids.iter().all(|i| i.len() == 32 && i.chars().all(|c| c.is_ascii_hexdigit())));
    }
}
