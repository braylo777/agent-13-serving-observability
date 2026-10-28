import { json, limited, body, cap } from "./core.js";
import { tracer, store, recent, summarize, PRICING } from "./obs.js";
import { serve } from "./serve.js";
import page from "./page.html";

export default {
  async fetch(req, env, ctx) {
    const url = new URL(req.url), p = url.pathname;
    const save = (t) => (ctx?.waitUntil ? ctx.waitUntil(store(env, t)) : store(env, t));
    if (p === "/api/metrics") { const ts = await recent(env); return json({ window: ts.length, routes: summarize(ts), pricing: PRICING }); }
    if (p === "/api/traces") return json((await recent(env)).slice(0, +(url.searchParams.get("n") || 25)));
    if (p.startsWith("/api/traces/")) { const t = (await recent(env)).find((x) => x.trace_id === p.split("/").pop()); return t ? json(t) : json({ error: "not found" }, 404); }
    // Non-model endpoint: traced the same way, used for synthetic load (no Workers AI).
    if (p === "/api/ping") {
      const tr = tracer("/api/ping");
      const v = await tr.span("work.hash", async () => { const d = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(url.searchParams.get("q") || "ping")); return [...new Uint8Array(d)].slice(0, 4).map((b) => b.toString(16)).join(""); });
      const t = tr.finish(200, { synthetic: true }); await save(t);
      return json({ ok: true, v, trace_id: t.trace_id }, 200);
    }
    if (req.method === "POST" && p === "/api/chat") {
      const tr = tracer("/api/chat");
      if (await tr.span("rate_limit", () => limited(req, env))) { const t = tr.finish(429); await save(t); return json({ error: "Rate limit: try again in a minute.", trace_id: t.trace_id }, 429); }
      const { prompt } = await body(req);
      if (!cap(prompt)) return json({ error: "prompt required (<=500 chars)" }, 400);
      try {
        const r = await serve(env, tr, prompt);
        const t = tr.finish(200, { tokens: r.tokens, cost: r.cost }); await save(t);
        return json({ ...r, trace_id: t.trace_id, duration_ms: t.duration_ms, spans: t.spans });
      } catch (e) { const t = tr.finish(502); await save(t); return json({ error: "Model error: " + e.message, trace_id: t.trace_id }, 502); }
    }
    return new Response(page, { headers: { "content-type": "text/html;charset=utf-8" } });
  },
};
