"""Routes. `host` supplies: ai(model, input), kv (TRACES binding or None), limited(), now() (ms, monotonic), epoch() (ms), trace_id(), sha_prefix(q)."""
from urllib.parse import parse_qs

from .obs import PRICING, Tracer, recent, store, summarize
from .serve import serve


def cap(s, n=500):
    return isinstance(s, str) and bool(s.strip()) and len(s) <= n


def _js_slice_end(n_raw, length):
    try:
        k = float(n_raw) if n_raw not in (None, "") else 25
    except ValueError:
        return 0
    if k != k:
        return 0
    k = int(k)
    return max(0, min(length, k if k >= 0 else length + k))


def handle(host, method, path, query="", body=None):
    """Returns (status, json); status 0 = serve the page."""
    qs = parse_qs(query, keep_blank_values=True)
    q = lambda k: (qs.get(k) or [None])[0]
    save = lambda t: store(host.kv, t)
    if path == "/api/metrics":
        ts = recent(host.kv)
        return 200, {"window": len(ts), "routes": summarize(ts), "pricing": PRICING}
    if path == "/api/traces":
        ts = recent(host.kv)
        return 200, ts[:_js_slice_end(q("n"), len(ts))]
    if path.startswith("/api/traces/"):
        t = next((x for x in recent(host.kv) if x["trace_id"] == path.split("/")[-1]), None)
        return (200, t) if t else (404, {"error": "not found"})
    if path == "/api/ping":
        tr = Tracer("/api/ping", host.now, host.trace_id(), host.epoch)
        v = tr.span("work.hash", lambda s: host.sha_prefix(q("q") or "ping"))
        t = tr.finish(200, synthetic=True)
        save(t)
        return 200, {"ok": True, "v": v, "trace_id": t["trace_id"]}
    if method == "POST" and path == "/api/chat":
        tr = Tracer("/api/chat", host.now, host.trace_id(), host.epoch)
        if tr.span("rate_limit", lambda s: host.limited()):
            t = tr.finish(429)
            save(t)
            return 429, {"error": "Rate limit: try again in a minute.", "trace_id": t["trace_id"]}
        prompt = body.get("prompt") if isinstance(body, dict) else None
        if not cap(prompt):
            return 400, {"error": "prompt required (<=500 chars)"}
        try:
            r = serve(host, tr, prompt)
            t = tr.finish(200, tokens=r["tokens"], cost=r["cost"])
            save(t)
            return 200, {**r, "trace_id": t["trace_id"], "duration_ms": t["duration_ms"], "spans": t["spans"]}
        except Exception as e:
            t = tr.finish(502)
            save(t)
            return 502, {"error": "Model error: " + str(e), "trace_id": t["trace_id"]}
    return 0, None
