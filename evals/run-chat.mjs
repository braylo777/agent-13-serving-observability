// Live chat eval: 20 prompts through /api/chat. Needs Workers AI quota.
import fs from "node:fs";
const BASE = process.env.BASE || "https://agent-13-serving-observability.braylo777.workers.dev";
const prompts = ["What is p95 latency?", "Explain a span vs a trace.", "How do neurons map to USD?", "Name two observability signals.", "What is a canary?",
  "Summarize KV vs Analytics Engine.", "Why rate-limit free tiers?", "Define TTFT.", "What is cold start?", "List three LLM cost drivers.",
  "What does a trace ID look like?", "Why estimate tokens?", "Name a Workers AI model.", "What is a dashboard?", "Define SLO.",
  "What is tail latency?", "Why store recent traces?", "Explain percentile.", "What is a binding?", "One sentence on free tier."];
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const pct = (xs, p) => { const s = [...xs].sort((a, b) => a - b); return s.length ? s[Math.min(s.length - 1, Math.ceil((p / 100) * s.length) - 1)] : null; };
const rows = [], lat = [];
for (const [i, prompt] of prompts.entries()) {
  const t0 = Date.now();
  const r = await fetch(BASE + "/api/chat", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ prompt }) });
  const body = await r.json().catch(() => ({}));
  const ms = Date.now() - t0; if (r.status === 200) lat.push(ms);
  rows.push({ i, prompt, status: r.status, ms, tokens: body.tokens, cost: body.cost, trace_id: body.trace_id, spans: body.spans?.map((s) => s.name) });
  console.log(i, r.status, ms); await sleep(13000);
}
const m = await (await fetch(BASE + "/api/metrics")).json();
const out = { project: "agent-13-serving-observability", eval: "live_chat", base: BASE, model: "@cf/meta/llama-3.3-70b-instruct-fp8-fast", run_at: new Date().toISOString(), n: rows.length,
  metrics: { latency_ms: { n: lat.length, p50: pct(lat, 50), p95: pct(lat, 95) }, server: m.routes["/api/chat"] || null, http_200: rows.filter((r) => r.status === 200).length }, cases: rows };
fs.writeFileSync(new URL("./results-chat.json", import.meta.url), JSON.stringify(out, null, 2));
console.log(JSON.stringify(out.metrics, null, 2));
