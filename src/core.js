// Shared helpers: Workers AI (free tier), rate limiting, JSON responses.
export const MODEL = "@cf/meta/llama-3.3-70b-instruct-fp8-fast";
export const EMBED = "@cf/baai/bge-base-en-v1.5";
export const json = (o, status = 200) => Response.json(o, { status });
export const text = (r) => (typeof r?.response === "string" ? r.response : JSON.stringify(r?.response ?? r));
export async function chat(env, messages, opts = {}) {
  return env.AI.run(opts.model || MODEL, { messages, max_tokens: opts.max_tokens || 400, ...(opts.tools ? { tools: opts.tools } : {}) });
}
export async function ask(env, system, user, opts) { return text(await chat(env, [{ role: "system", content: system }, { role: "user", content: user }], opts)); }
export function parseJSON(s, fallback = null) {
  const m = String(s).match(/[\[{][\s\S]*[\]}]/);
  try { return m ? JSON.parse(m[0]) : fallback; } catch { return fallback; }
}
// Free-tier guard: per-IP limit via the Workers Rate Limiting binding.
export async function limited(req, env) {
  const ip = req.headers.get("cf-connecting-ip") || "anon";
  return env.RATE_LIMITER && !(await env.RATE_LIMITER.limit({ key: ip })).success;
}
export async function body(req) { return req.json().catch(() => ({})); }
export function cap(s, n = 500) { return typeof s === "string" && s.trim() && s.length <= n; }
