import json, re
import pytest
from agent13.obs import Tracer, cost_estimate, pct, summarize, est_tokens, store, recent
from agent13.api import handle

MODEL = "@cf/meta/llama-3.3-70b-instruct-fp8-fast"


class KV:
    def __init__(self): self.m = {}
    def get(self, k): return self.m.get(k)
    def put(self, k, v): self.m[k] = v
    def delete(self, k): self.m.pop(k, None)


class Host:
    def __init__(self, ai=None, kv=None, limited=False):
        self._ai, self.kv, self._lim, self.n, self.c = ai, kv, limited, 0, [0]
    def ai(self, m, i): return self._ai(m, i)
    def limited(self): return self._lim
    def now(self): self.c[0] += 5; return self.c[0]
    def epoch(self): return 1_700_000_000_000
    def trace_id(self): self.n += 1; return f"{self.n:032x}"
    def sha_prefix(self, q): return "abcd"


fake_ai = lambda usage={"prompt_tokens": 40, "completion_tokens": 20}: (lambda m, i: {"response": "hi", "usage": usage})


def tracer(route="r", now=None):
    t = [0]
    def clock(): t[0] += 5; return t[0]
    return Tracer(route, now or clock, "0" * 32, lambda: 0)


def test_spans_order_timing_errors():
    tr = tracer()
    tr.span("a", lambda s: 1)
    with pytest.raises(RuntimeError):
        tr.span("b", lambda s: (_ for _ in ()).throw(RuntimeError("boom")))
    f = tr.finish(500)
    assert [s["name"] for s in f["spans"]] == ["a", "b"] and f["spans"][1]["error"] == "boom" and all(s["duration_ms"] > 0 for s in f["spans"])


def test_cost_estimate_and_edges():
    c = cost_estimate(MODEL, 1e6, 0)
    assert c["neurons"] == 26668 and c["usd"] == 0.293348 and "estimate" in c["label"]
    assert cost_estimate(MODEL, 0, 1e6)["neurons"] == 204805 and cost_estimate("unknown", 1, 1) is None
    z = cost_estimate(MODEL, 0, 0)
    assert z["neurons"] == 0 and z["usd"] == 0


def test_percentiles_and_token_estimate():
    xs = list(range(1, 101))
    assert pct(xs, 50) == 50 and pct(xs, 95) == 95 and pct([], 50) is None and pct([7], 95) == 7
    assert pct([5, 1, 3, 2, 4], 50) == 3 and pct([5, 1, 3, 2, 4], 100) == 5
    assert est_tokens("abcdefgh") == 2 and est_tokens("") == 0 and est_tokens("a " * 100) >= est_tokens("a ")


def test_summarize_groups_and_counts_errors():
    s = summarize([{"route": "/a", "status": 200, "duration_ms": 10}, {"route": "/a", "status": 502, "duration_ms": 30}, {"route": "/b", "status": 200, "duration_ms": 5, "tokens": {"in": 10, "out": 4}, "cost": {"usd": 0.001}}])
    assert s["/a"]["n"] == 2 and s["/a"]["errors"] == 1 and s["/a"]["p95_ms"] == 30 and s["/b"]["avg_tokens_in"] == 10 and s["/b"]["est_cost_usd_per_req"] == 0.001


def test_kv_store_newest_first_caps_200():
    kv = KV()
    for i in range(205):
        store(kv, {"trace_id": f"id{i}", "i": i})
    r = recent(kv)
    assert len(r) == 200 and r[0]["i"] == 204 and r[-1]["i"] == 5


def test_http_chat_tokens_cost_spans_trace():
    host = Host(fake_ai(), KV())
    s, j = handle(host, "POST", "/api/chat", "", {"prompt": "hello"})
    assert s == 200 and j["tokens"]["in"] == 40 and j["tokens"]["source"] == "model usage" and j["cost"]["usd"] > 0
    assert [x["name"] for x in j["spans"]] == ["rate_limit", "build_prompt", "model.generate"]
    assert handle(host, "GET", "/api/traces/" + j["trace_id"])[1]["trace_id"] == j["trace_id"]


def test_http_fallbacks_failure_and_validation():
    host = Host(lambda m, i: {"response": "abcd"}, KV())
    j = handle(host, "POST", "/api/chat", "", {"prompt": "hi"})[1]
    assert j["tokens"]["source"] == "chars/4 estimate" and j["tokens"]["out"] == 1
    def boom(m, i): raise RuntimeError("quota")
    host = Host(boom, KV())
    assert handle(host, "POST", "/api/chat", "", {"prompt": "hi"})[0] == 502
    assert handle(host, "GET", "/api/metrics")[1]["routes"]["/api/chat"]["errors"] == 1 and recent(host.kv)[0]["spans"][-1]["error"] == "quota"
    assert handle(Host(kv=KV()), "POST", "/api/chat", "", {})[0] == 400
    lim = Host(kv=KV(), limited=True)
    assert handle(lim, "POST", "/api/chat", "", {"prompt": "x"})[0] == 429
    assert handle(lim, "GET", "/api/ping", "")[1]["ok"] and handle(lim, "GET", "/api/traces/nope")[0] == 404 and handle(lim, "GET", "/")[0] == 0


def test_trace_ids_unique_and_hex():
    h = Host()
    ids = {h.trace_id() for _ in range(1000)}
    assert len(ids) == 1000 and all(re.fullmatch(r"[0-9a-f]{32}", i) for i in ids)
