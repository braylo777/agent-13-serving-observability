import { test } from "node:test";
import assert from "node:assert/strict";
import { tracer, costEstimate, pct, estTokens } from "../src/obs.js";
test("edge: percentile of unsorted input", () => { assert.equal(pct([5, 1, 3, 2, 4], 50), 3); assert.equal(pct([5, 1, 3, 2, 4], 100), 5); });
test("edge: zero tokens cost zero", () => { const c = costEstimate("@cf/meta/llama-3.3-70b-instruct-fp8-fast", 0, 0); assert.equal(c.neurons, 0); assert.equal(c.usd, 0); });
test("edge: token estimate monotonic in length", () => { assert.ok(estTokens("a ".repeat(100)) >= estTokens("a ")); });
test("edge: 1000 trace ids unique", () => { const s = new Set(Array.from({ length: 1000 }, () => tracer("r").trace_id)); assert.equal(s.size, 1000); });
