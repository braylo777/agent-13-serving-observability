// Parity helper: same scenario as the legacy JS harness; prints JSON.
use agent_13_serving_observability::{api::*, obs::*};
use futures::executor::block_on;
use serde_json::{json, Value};
use std::{cell::{Cell, RefCell}, collections::HashMap};

struct H { resp: Value, kv: RefCell<HashMap<String, String>> }
impl Kv for H {
    async fn get(&self, k: &str) -> Option<String> { self.kv.borrow().get(k).cloned() }
    async fn put(&self, k: &str, v: &str) { self.kv.borrow_mut().insert(k.into(), v.into()); }
    async fn delete(&self, k: &str) { self.kv.borrow_mut().remove(k); }
}
impl Host for H {
    fn has_kv(&self) -> bool { true }
    async fn ai(&self, _m: &str, _i: Value) -> Result<Value, String> { Ok(self.resp.clone()) }
    async fn limited(&self) -> bool { false }
    fn now(&self) -> f64 { 0.0 }
    fn epoch(&self) -> i64 { 0 }
    fn trace_id(&self) -> String { "x".into() }
    fn sha_prefix(&self, _q: &str) -> String { "x".into() }
}
fn run(resp: Value) -> Value {
    let h = H { resp, kv: Default::default() };
    let c = Cell::new(0.0);
    let clk = || { c.set(c.get() + 2.345); c.get() };
    let mut tr = Tracer::new("/api/chat", &clk, "x".into(), 0);
    let r = block_on(serve(&h, &mut tr, "hello")).unwrap();
    let mut f = tr.finish(200, json!({"tokens": r["tokens"], "cost": r["cost"]}));
    let o = f.as_object_mut().unwrap();
    o.remove("trace_id");
    o.remove("ts");
    json!({"r": r, "f": f})
}
fn main() {
    let mut out = serde_json::Map::new();
    out.insert("cost".into(), Value::Array([(1e6, 0.0), (0.0, 1e6), (0.0, 0.0), (40.0, 20.0), (1.0, 1.0), (123.0, 456.0), (7.0, 0.0), (0.0, 3.0), (1e6, 1e6), (33.0, 77.0), (5.0, 5.0), (2048.0, 512.0), (1e-3, 0.0)].iter().map(|(i, o)| cost_estimate(MODEL, *i, *o)).chain([cost_estimate("unknown", 1.0, 1.0)]).collect()));
    let ten: Vec<f64> = (1..=10).map(|i| i as f64).collect();
    out.insert("pct".into(), json!([pct(&ten, 50.0), pct(&ten, 95.0), pct(&[5.0, 1.0, 3.0, 2.0, 4.0], 50.0), pct(&[], 50.0), pct(&[7.0], 95.0)]));
    out.insert("est".into(), json!(["abcdefgh", "", "é日本語😀 hello", &"a".repeat(101)].iter().map(|s| est_tokens(s)).collect::<Vec<_>>()));
    out.insert("sum".into(), summarize(&[json!({"route":"/a","status":200,"duration_ms":10}), json!({"route":"/a","status":502,"duration_ms":30}), json!({"route":"/b","status":200,"duration_ms":5,"tokens":{"in":10,"out":4},"cost":{"usd":0.001}}), json!({"route":"/b","status":200,"duration_ms":9,"tokens":{"in":13,"out":7},"cost":{"usd":0.0023}}), json!({"route":"/b","status":429,"duration_ms":1}), json!({"route":"/c","status":200,"duration_ms":2.5,"tokens":{"in":3,"out":1},"cost":{"usd":0.00000123456789}})]));
    let c = Cell::new(0.0);
    let clk = || { c.set(c.get() + 3.7); c.get() };
    let mut tr = Tracer::new("r", &clk, "x".into(), 0);
    let (s, _) = tr.start(); tr.end("a", s, json!({"x": 1}), None);
    let (s, _) = tr.start(); tr.end("b", s, json!({}), Some("boom".into()));
    let mut f = tr.finish(500, json!({"k": 1}));
    f.as_object_mut().unwrap().remove("trace_id"); f.as_object_mut().unwrap().remove("ts");
    out.insert("tr".into(), f);
    out.insert("serve".into(), json!([run(json!({"response":"hi","usage":{"prompt_tokens":40,"completion_tokens":20}})), run(json!({"response":"abcd"})), run(json!({"response":{"a":1}})), run(json!({"response":"x","usage":{"prompt_tokens":"z"}}))]));
    let h = H { resp: json!({}), kv: Default::default() };
    for i in 0..205 { block_on(store(&h, &json!({"trace_id": format!("id{i}"), "i": i}))); }
    let r = block_on(recent(&h));
    out.insert("store".into(), json!({"n": r.len(), "first": r[0]["i"], "last": r.last().unwrap()["i"], "keys": h.kv.borrow().len()}));
    println!("{}", serde_json::to_string(&Value::Object(out)).unwrap());
}
