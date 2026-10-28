// Bundles src/ into one ES module (dist/worker.js) for API upload. wrangler users don't need this.
import fs from "node:fs";
const strip = (f) => fs.readFileSync(f, "utf8").replace(/^import .*\n/gm, "").replace(/^export default/m, "export default");
const libs = fs.readdirSync("src").filter((f) => f.endsWith(".js") && f !== "index.js").sort((a, b) => (a === "core.js" ? -1 : b === "core.js" ? 1 : a.localeCompare(b)));
const html = fs.readFileSync("src/page.html", "utf8");
let out = libs.map((f) => strip("src/" + f)).join("\n") + `\nconst page = ${JSON.stringify(html)};\n` + strip("src/index.js");
fs.mkdirSync("dist", { recursive: true });
fs.writeFileSync("dist/worker.js", out);
