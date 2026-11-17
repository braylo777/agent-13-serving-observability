"""One traced model call: build prompt -> Workers AI -> parse usage -> cost."""
import json
from .obs import MODEL, cost_estimate, est_tokens


def text(r):
    v = r.get("response") if isinstance(r, dict) else None
    return v if isinstance(v, str) else json.dumps(v if v is not None else r, separators=(",", ":"), ensure_ascii=False)


def serve(host, tr, prompt):
    messages = tr.span("build_prompt", lambda s: [{"role": "system", "content": "You are a concise assistant. Answer in under 80 words."}, {"role": "user", "content": prompt}])
    def gen(s):
        s["attrs"]["model"] = MODEL
        return host.ai(MODEL, {"messages": messages, "max_tokens": 200})
    r = tr.span("model.generate", gen)
    answer = text(r)
    u = r.get("usage") if isinstance(r, dict) else None
    measured = isinstance(u, dict) and isinstance(u.get("prompt_tokens"), (int, float)) and not isinstance(u.get("prompt_tokens"), bool)
    if measured:
        tokens = {"in": u["prompt_tokens"], "out": u.get("completion_tokens"), "source": "model usage"}
    else:
        tokens = {"in": est_tokens(" ".join(m["content"] for m in messages)), "out": est_tokens(answer), "source": "chars/4 estimate"}
    return {"answer": answer, "tokens": tokens, "cost": cost_estimate(MODEL, tokens["in"], tokens["out"])}
