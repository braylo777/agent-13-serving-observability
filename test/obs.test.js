import { test } from "node:test";
import assert from "node:assert/strict";
import { tracer, costEstimate, pct, summarize, estTokens, store, recent } from "../src/obs.js";
import worker from "../src/index.js";
const kv = () => { const m = new Map(); return { get: async (k, t) => (m.has(k) ? (t === "json" ? JSON.parse(m.get(k)) : m.get(k)) : null), put: async (k, v) => m.set(k, v), delete: async (k) => m.delete(k) }; };
const fakeAI = (usage = { prompt_tokens: 40, completion_tokens: 20 }) => ({ run: async () => ({ response: "hi", usage }) });

test("trace ids are 32 hex chars and unique", () => { const a = tracer("r").trace_id, b = tracer("r").trace_id; assert.match(a, /^[0-9a-f]{32}$/); assert.notEqual(a, b); });
test("spans record order, timing and errors", async () => {
  let t = 0; const tr = tracer("r", () => (t += 5));
  await tr.span("a", async () => 1);
  await assert.rejects(tr.span("b", async () => { throw new Error("boom"); }));
  const f = tr.finish(500); assert.deepEqual(f.spans.map((s) => s.name), ["a", "b"]); assert.equal(f.spans[1].error, "boom"); assert.ok(f.spans.every((s) => s.duration_ms > 0));
});
test("cost estimate uses published neuron pricing", () => {
  const c = costEstimate("@cf/meta/llama-3.3-70b-instruct-fp8-fast", 1e6, 0); assert.equal(c.neurons, 26668); assert.equal(c.usd, 0.293348);
  const o = costEstimate("@cf/meta/llama-3.3-70b-instruct-fp8-fast", 0, 1e6); assert.equal(o.neurons, 204805);
  assert.equal(costEstimate("unknown", 1, 1), null); assert.match(c.label, /estimate/);
});
test("percentiles (nearest-rank)", () => { const xs = Array.from({ length: 100 }, (_, i) => i + 1); assert.equal(pct(xs, 50), 50); assert.equal(pct(xs, 95), 95); assert.equal(pct([], 50), null); assert.equal(pct([7], 95), 7); });
test("token estimate fallback", () => { assert.equal(estTokens("abcdefgh"), 2); assert.equal(estTokens(""), 0); });
test("summarize groups by route and counts errors", () => {
  const s = summarize([{ route: "/a", status: 200, duration_ms: 10 }, { route: "/a", status: 502, duration_ms: 30 }, { route: "/b", status: 200, duration_ms: 5, tokens: { in: 10, out: 4 }, cost: { usd: 0.001 } }]);
  assert.equal(s["/a"].n, 2); assert.equal(s["/a"].errors, 1); assert.equal(s["/a"].p95_ms, 30); assert.equal(s["/b"].avg_tokens_in, 10); assert.equal(s["/b"].est_cost_usd_per_req, 0.001);
});
test("KV store keeps newest first and caps at 200", async () => { const env = { TRACES: kv() }; for (let i = 0; i < 205; i++) await store(env, { trace_id: "id" + i, i }); const r = await recent(env); assert.equal(r.length, 200); assert.equal(r[0].i, 204); assert.equal(r.at(-1).i, 5); });
test("HTTP /api/chat returns answer, tokens from usage, cost, spans and stores trace", async () => {
  const env = { AI: fakeAI(), TRACES: kv() };
  const r = await worker.fetch(new Request("https://x/api/chat", { method: "POST", body: JSON.stringify({ prompt: "hello" }) }), env);
  const j = await r.json(); assert.equal(r.status, 200); assert.equal(j.tokens.in, 40); assert.equal(j.tokens.source, "model usage"); assert.ok(j.cost.usd > 0);
  assert.deepEqual(j.spans.map((s) => s.name), ["rate_limit", "build_prompt", "model.generate"]);
  const t = await (await worker.fetch(new Request("https://x/api/traces/" + j.trace_id), env)).json(); assert.equal(t.trace_id, j.trace_id);
});
test("HTTP falls back to chars/4 when model has no usage", async () => {
  const env = { AI: { run: async () => ({ response: "abcd" }) }, TRACES: kv() };
  const j = await (await worker.fetch(new Request("https://x/api/chat", { method: "POST", body: JSON.stringify({ prompt: "hi" }) }), env)).json();
  assert.equal(j.tokens.source, "chars/4 estimate"); assert.equal(j.tokens.out, 1);
});
test("HTTP model failure -> 502 with a stored error trace", async () => {
  const env = { AI: { run: async () => { throw new Error("quota"); } }, TRACES: kv() };
  const r = await worker.fetch(new Request("https://x/api/chat", { method: "POST", body: JSON.stringify({ prompt: "hi" }) }), env); assert.equal(r.status, 502);
  const m = await (await worker.fetch(new Request("https://x/api/metrics"), env)).json(); assert.equal(m.routes["/api/chat"].errors, 1);
  const tr = (await recent(env))[0]; assert.equal(tr.spans.at(-1).error, "quota");
});
test("HTTP validation, rate limit, ping and 404", async () => {
  const env = { TRACES: kv(), RATE_LIMITER: { limit: async () => ({ success: false }) } };
  assert.equal((await worker.fetch(new Request("https://x/api/chat", { method: "POST", body: "{}" }), {})).status, 400);
  assert.equal((await worker.fetch(new Request("https://x/api/chat", { method: "POST", body: JSON.stringify({ prompt: "x" }) }), env)).status, 429);
  const p = await (await worker.fetch(new Request("https://x/api/ping"), env)).json(); assert.ok(p.ok);
  assert.equal((await worker.fetch(new Request("https://x/api/traces/nope"), env)).status, 404);
  assert.match(await (await worker.fetch(new Request("https://x/"), env)).text(), /Observability/);
});
