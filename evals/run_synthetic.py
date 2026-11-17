"""Synthetic load against /api/ping (no Workers AI). Measures p50/p95 and confirms tracing.
Usage: BASE=https://... N=40 python3 evals/run_synthetic.py"""
import json, math, os, time, urllib.request
from datetime import datetime, timezone
from pathlib import Path

BASE = os.environ.get("BASE", "https://agent-13-serving-observability.braylo777.workers.dev")
N = int(os.environ.get("N", "40"))
pct = lambda xs, p: sorted(xs)[min(len(xs) - 1, math.ceil(p / 100 * len(xs)) - 1)]
get = lambda u: json.load(urllib.request.urlopen(u, timeout=60))
lat, ids = [], []
for i in range(N):
    t0 = time.time()
    j = get(f"{BASE}/api/ping?q={i}")
    lat.append(round((time.time() - t0) * 1000)); ids.append(j["trace_id"])
    if (i + 1) % 10 == 0:
        print(i + 1, "ok", flush=True)
    time.sleep(0.05)
time.sleep(0.5)
m = get(BASE + "/api/metrics")
t = get(BASE + "/api/traces?n=5")
out = {"project": "agent-13-serving-observability", "eval": "synthetic_ping_load", "base": BASE, "run_at": datetime.now(timezone.utc).isoformat(timespec="milliseconds").replace("+00:00", "Z"), "n": N,
       "metrics": {"client_latency_ms": {"n": len(lat), "p50": pct(lat, 50), "p95": pct(lat, 95), "note": "end-to-end /api/ping from the eval client"},
                   "server_reported": (m.get("routes") or {}).get("/api/ping"), "unique_trace_ids": len(set(ids)), "sample_spans": (t[0].get("spans") if t else None)},
       "chat_eval": {"status": "not yet run", "reason": "Workers AI free daily neuron quota exhausted until ~00:00 UTC Oct 28", "rerun": f"BASE={BASE} python3 evals/run_chat.py"},
       "notes": "Ping path exercises the same tracer, KV store and dashboard as /api/chat; no model tokens or cost on this path."}
(Path(__file__).parent / "results.json").write_text(json.dumps(out, indent=2))
print(json.dumps(out["metrics"], indent=2))
