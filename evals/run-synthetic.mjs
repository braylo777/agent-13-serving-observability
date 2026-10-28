// Synthetic load against /api/ping (no Workers AI). Measures p50/p95 and confirms tracing.
import fs from "node:fs";
const BASE = process.env.BASE || "https://agent-13-serving-observability.braylo777.workers.dev";
const N = +(process.env.N || 40);
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const pct = (xs, p) => { const s = [...xs].sort((a, b) => a - b); return s[Math.min(s.length - 1, Math.ceil((p / 100) * s.length) - 1)]; };
const lat = [], ids = [];
for (let i = 0; i < N; i++) {
  const t0 = Date.now();
  const r = await fetch(BASE + "/api/ping?q=" + i);
  const j = await r.json();
  lat.push(Date.now() - t0); ids.push(j.trace_id);
  if ((i + 1) % 10 === 0) console.log(i + 1, "ok");
  await sleep(50);
}
await sleep(500);
const m = await (await fetch(BASE + "/api/metrics")).json();
const t = await (await fetch(BASE + "/api/traces?n=5")).json();
const out = { project: "agent-13-serving-observability", eval: "synthetic_ping_load", base: BASE, run_at: new Date().toISOString(), n: N,
  metrics: {
    client_latency_ms: { n: lat.length, p50: pct(lat, 50), p95: pct(lat, 95), note: "end-to-end /api/ping from the eval client" },
    server_reported: m.routes["/api/ping"] || null,
    unique_trace_ids: new Set(ids).size,
    sample_spans: t[0]?.spans || null,
  },
  chat_eval: { status: "not yet run", reason: "Workers AI free daily neuron quota exhausted until ~00:00 UTC Oct 28 (~7 PM CT Oct 27 → reset next day ~7 PM CT Oct 28)", rerun: "BASE=https://agent-13-serving-observability.braylo777.workers.dev node evals/run-chat.mjs" },
  notes: "Ping path exercises the same tracer, KV store and dashboard as /api/chat; no model tokens or cost on this path." };
fs.writeFileSync(new URL("./results.json", import.meta.url), JSON.stringify(out, null, 2));
console.log(JSON.stringify(out.metrics, null, 2));
