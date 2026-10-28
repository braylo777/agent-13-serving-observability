import { MODEL, text } from "./core.js";
import { estTokens, costEstimate } from "./obs.js";
// One traced model call: build prompt -> Workers AI -> parse usage -> cost.
export async function serve(env, tr, prompt) {
  const messages = await tr.span("build_prompt", async () => [{ role: "system", content: "You are a concise assistant. Answer in under 80 words." }, { role: "user", content: prompt }]);
  const r = await tr.span("model.generate", async (s) => { s.attrs.model = MODEL; return env.AI.run(MODEL, { messages, max_tokens: 200 }); });
  const answer = text(r);
  const measured = r?.usage && Number.isFinite(r.usage.prompt_tokens);
  const tokens = measured ? { in: r.usage.prompt_tokens, out: r.usage.completion_tokens, source: "model usage" } : { in: estTokens(messages.map((m) => m.content).join(" ")), out: estTokens(answer), source: "chars/4 estimate" };
  return { answer, tokens, cost: costEstimate(MODEL, tokens.in, tokens.out) };
}
