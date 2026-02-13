use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug)]
pub struct BuiltinSig {
    pub arity_min: usize,
    pub value_first: bool,
}

fn ins(m: &mut BTreeMap<String, BuiltinSig>, module: &str, export: &str, arity_min: usize, value_first: bool) {
    let sig = BuiltinSig { arity_min, value_first };
        m.insert(format!("{module}::{export}"), sig);
}

pub fn builtin_sig_table() -> BTreeMap<String, BuiltinSig> {
    let mut m: BTreeMap<String, BuiltinSig> = BTreeMap::new();

    // std/list
    ins(&mut m, "std/list", "uniqueBy", 2, true);
    ins(&mut m, "std/list", "take", 2, true);
    ins(&mut m, "std/list", "stableSortBy", 2, true);
    ins(&mut m, "std/list", "sortBy", 2, true);
    ins(&mut m, "std/list", "slice", 3, true);
    ins(&mut m, "std/list", "push", 2, true);
    ins(&mut m, "std/list", "map", 2, true);
    ins(&mut m, "std/list", "groupBy", 2, true);
    ins(&mut m, "std/list", "fold", 3, true);
    ins(&mut m, "std/list", "flatMap", 2, true);
    ins(&mut m, "std/list", "filter", 2, true);
    ins(&mut m, "std/list", "drop", 2, true);
    ins(&mut m, "std/list", "chunk", 2, true);
    ins(&mut m, "std/list", "get", 2, true);
    ins(&mut m, "std/list", "sort_by_int_key", 2, true);
    ins(&mut m, "std/list", "sort_int", 1, true);
    ins(&mut m, "std/list", "dedupe_sorted_int", 1, true);
    ins(&mut m, "std/list", "hist_int", 1, true);

    // std/result
    ins(&mut m, "std/result", "ok", 1, false);
    ins(&mut m, "std/result", "andThen", 2, true);
    ins(&mut m, "std/result", "err", 1, false);

    // std/grow
    ins(&mut m, "std/grow", "unfold_tree", 2, true);
    ins(&mut m, "std/grow", "unfold", 2, true);

    // std/flow
    ins(&mut m, "std/flow", "pipe", 2, true);

    // std/str
    ins(&mut m, "std/str", "len", 1, true);
    ins(&mut m, "std/str", "concat", 2, true);

    // std/map
    ins(&mut m, "std/map", "get", 2, true);
    ins(&mut m, "std/map", "set", 3, true);

    // std/rec
    ins(&mut m, "std/rec", "empty", 0, false);
    ins(&mut m, "std/rec", "keys", 1, true);
    ins(&mut m, "std/rec", "values", 1, true);
    ins(&mut m, "std/rec", "has", 2, true);
    ins(&mut m, "std/rec", "get", 2, true);
    ins(&mut m, "std/rec", "getOr", 3, true);
    ins(&mut m, "std/rec", "getOrErr", 3, true);
    ins(&mut m, "std/rec", "set", 3, true);
    ins(&mut m, "std/rec", "remove", 2, true);
    ins(&mut m, "std/rec", "merge", 2, true);
    ins(&mut m, "std/rec", "select", 2, true);
    ins(&mut m, "std/rec", "rename", 3, true);
    ins(&mut m, "std/rec", "update", 3, true);

    // std/fs (ontology Stage allowlist currently expects this)
    // Value-first: bytes/value first, then path/opts (minimum 2 args).
    ins(&mut m, "std/fs", "writeAll", 2, true);

    // std/http
    ins(&mut m, "std/http", "okOr", 2, true);
    ins(&mut m, "std/http", "post", 2, true);

    // std/int
    ins(&mut m, "std/int", "clamp", 3, true);

    // std/json
    ins(&mut m, "std/json", "pathGet", 2, true);
    ins(&mut m, "std/json", "pathSet", 3, true);

    // std/null
    ins(&mut m, "std/null", "guardNotNull", 2, true);
    ins(&mut m, "std/null", "coalesce", 2, true);

  // BEGIN AUTOGEN STAGE SIG STUBS
    ins(&mut m, "std/option", "andThen", 2, true);
    ins(&mut m, "std/option", "map", 2, true);
    ins(&mut m, "std/option", "toResult", 2, true);
    ins(&mut m, "std/option", "unwrapOr", 2, true);
    ins(&mut m, "std/option", "unwrapOrElse", 2, true);

    ins(&mut m, "std/path", "joinAll", 1, true);

    ins(&mut m, "std/result", "map", 2, true);
    ins(&mut m, "std/result", "mapErr", 2, true);
    ins(&mut m, "std/result", "orElse", 2, true);
    ins(&mut m, "std/result", "unwrapOr", 2, true);
    ins(&mut m, "std/result", "unwrapOrElse", 2, true);

    ins(&mut m, "std/trace", "emit", 2, true);
    ins(&mut m, "std/trace", "artifact_in", 2, true);
    ins(&mut m, "std/trace", "artifact_out", 2, true);
    ins(&mut m, "std/trace", "module_graph", 1, true);

    ins(&mut m, "std/artifact", "in", 2, true);
    ins(&mut m, "std/artifact", "out", 2, true);
    ins(&mut m, "std/artifact", "bytes", 1, true);
    ins(&mut m, "std/artifact", "cid_of_bytes", 1, true);

    ins(&mut m, "std/hash", "sha256_bytes", 1, true);
    ins(&mut m, "std/hash", "sha256_text", 1, true);
    ins(&mut m, "std/hash", "is_sha256", 1, true);
    ins(&mut m, "std/hash", "cid_hex", 1, true);

    ins(&mut m, "std/schema", "check", 2, true);

    ins(&mut m, "std/str", "contains", 2, true);
    ins(&mut m, "std/str", "endsWith", 2, true);
    ins(&mut m, "std/str", "join", 2, true);
    ins(&mut m, "std/str", "padLeft", 2, true);
    ins(&mut m, "std/str", "padRight", 2, true);
    ins(&mut m, "std/str", "replace", 3, true);
    ins(&mut m, "std/str", "slice", 3, true);
    ins(&mut m, "std/str", "split", 2, true);
    ins(&mut m, "std/str", "startsWith", 2, true);

    ins(&mut m, "std/time", "add", 2, true);
    ins(&mut m, "std/time", "sub", 2, true);
  // END AUTOGEN STAGE SIG STUBS
    m
}
