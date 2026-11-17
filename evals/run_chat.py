"""Live chat eval: 20 prompts through /api/chat. Needs Workers AI quota. Usage: BASE=https://... python3 evals/run_chat.py"""
import json, math, os, time, urllib.error, urllib.request
from datetime import datetime, timezone
from pathlib import Path

BASE = os.environ.get("BASE", "https://agent-13-serving-observability.braylo777.workers.dev")
prompts = ["What is p95 latency?", "Explain a span vs a trace.", "How do neurons map to USD?", "Name two observability signals.", "What is a canary?",
           "Summarize KV vs Analytics Engine.", "Why rate-limit free tiers?", "Define TTFT.", "What is cold start?", "List three LLM cost drivers.",
           "What does a trace ID look like?", "Why estimate tokens?", "Name a Workers AI model.", "What is a dashboard?", "Define SLO.",
           "What is tail latency?", "Why store recent traces?", "Explain percentile.", "What is a binding?", "One sentence on free tier."]
pct = lambda xs, p: sorted(xs)[min(len(xs) - 1, math.ceil(p / 100 * len(xs)) - 1)] if xs else None


def req(url, body=None):
    r = urllib.request.Request(url, json.dumps(body).encode() if body is not None else None, {"content-type": "application/json"})
    try:
        x = urllib.request.urlopen(r, timeout=60); status, raw = x.status, x.read()
    except urllib.error.HTTPError as e:
        status, raw = e.code, e.read()
    try:
        return status, json.loads(raw)
    except ValueError:
        return status, {}


rows, lat = [], []
for i, prompt in enumerate(prompts):
    t0 = time.time()
    status, body = req(BASE + "/api/chat", {"prompt": prompt})
    ms = round((time.time() - t0) * 1000)
    if status == 200:
        lat.append(ms)
    rows.append({"i": i, "prompt": prompt, "status": status, "ms": ms, "tokens": body.get("tokens"), "cost": body.get("cost"), "trace_id": body.get("trace_id"), "spans": [s["name"] for s in body.get("spans", [])] if body.get("spans") else None})
    print(i, status, ms, flush=True)
    time.sleep(13)
m = req(BASE + "/api/metrics")[1]
out = {"project": "agent-13-serving-observability", "eval": "live_chat", "base": BASE, "model": "@cf/meta/llama-3.3-70b-instruct-fp8-fast", "run_at": datetime.now(timezone.utc).isoformat(timespec="milliseconds").replace("+00:00", "Z"), "n": len(rows),
       "metrics": {"latency_ms": {"n": len(lat), "p50": pct(lat, 50), "p95": pct(lat, 95)}, "server": (m.get("routes") or {}).get("/api/chat"), "http_200": sum(1 for r in rows if r["status"] == 200)}, "cases": rows}
(Path(__file__).parent / "results-chat.json").write_text(json.dumps(out, indent=2))
print(json.dumps(out["metrics"], indent=2))
