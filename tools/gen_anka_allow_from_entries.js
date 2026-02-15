const fs = require("fs");
const path = require("path");

function main() {
  const src = process.argv[2] || "ontology/stdlib_surface.v1_0.ontology.json";
  const dst = process.argv[3] || "spec/v1_0/anka_policy_allowed_stdlib.v1.json";

  const v = JSON.parse(fs.readFileSync(src, "utf8"));
  if (!v || typeof v !== "object") throw new Error("bad json");
  if (v.schema !== "fard.stdlib_surface.entries.v1_0") {
    throw new Error("unexpected schema: " + v.schema);
  }
  const entries = v.entries;
  if (!Array.isArray(entries)) throw new Error("missing entries array");

  const mods = new Map(); // module -> Set(exports)
  for (const e of entries) {
    if (!e || typeof e !== "object") throw new Error("bad entry");
    const m = e.module;
    const ex = e.export;
    if (typeof m !== "string" || typeof ex !== "string") throw new Error("bad entry fields");
    if (!mods.has(m)) mods.set(m, new Set());
    mods.get(m).add(ex);
  }

  const modules = {};
  for (const m of Array.from(mods.keys()).sort()) {
    modules[m] = Array.from(mods.get(m)).sort();
  }

  const out = {
    schema: "fard.anka.policy.allowed_stdlib.v1",
    source: src,
    modules
  };

  fs.mkdirSync(path.dirname(dst), { recursive: true });
  fs.writeFileSync(dst, JSON.stringify(out), "utf8");
  console.log("WROTE", dst, "modules_len", Object.keys(modules).length);
}

main();
