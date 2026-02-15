const fs = require("fs");
const path = require("path");

function die(msg) {
  throw new Error(msg);
}

function main() {
  const src = process.argv[2] || "ontology/stdlib_surface.v1_0.ontology.json";
  const dst = process.argv[3] || "spec/v1_0/anka_policy_allowed_stdlib.v1.json";

  const v = JSON.parse(fs.readFileSync(src, "utf8"));
  if (!v || typeof v !== "object") die("bad json");
  if (v.schema !== "fard.stdlib_surface.entries.v1_0") die("unexpected schema: " + v.schema);
  if (!Array.isArray(v.entries)) die("missing entries");

  const requiredModules = [
    "std/hash",
    "std/bytes",
    "std/codec",
    "std/json",
    "std/str",
    "std/record",
    "std/list",
    "std/result",
    "std/option",
    "std/trace",
    "std/artifact",
    "std/time",
    "std/fs",
    "std/http"
  ];

  const requiredSet = new Set(requiredModules);

  const mods = new Map();
  for (const e of v.entries) {
    if (!e || typeof e !== "object") die("bad entry");
    const m = e.module;
    const ex = e.export;
    if (typeof m !== "string" || typeof ex !== "string") die("bad entry fields");
    if (!requiredSet.has(m)) continue;
    if (!mods.has(m)) mods.set(m, new Set());
    mods.get(m).add(ex);
  }

  for (const m of requiredModules) {
    if (!mods.has(m)) die("ANKA required module missing from surface: " + m);
    if (mods.get(m).size === 0) die("ANKA required module has empty exports in surface: " + m);
  }

  const modules = {};
  for (const m of requiredModules.slice().sort()) {
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
