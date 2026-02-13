use std::collections::{BTreeMap, BTreeSet};

fn expect_only_keys(
    obj: &serde_json::Map<String, serde_json::Value>,
    allowed: &[&str],
) -> Result<(), String> {
    let allow: BTreeSet<&str> = allowed.iter().copied().collect();
    for k in obj.keys() {
        if !allow.contains(k.as_str()) {
            return Err(format!("M4_EXTRA_KEY {}", k));
        }
    }
    Ok(())
}

fn expect_str<'a>(
    obj: &'a serde_json::Map<String, serde_json::Value>,
    k: &str,
) -> Result<&'a str, String> {
    obj.get(k)
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("M4_EXPECT_STRING {}", k))
}

fn expect_obj<'a>(
    obj: &'a serde_json::Map<String, serde_json::Value>,
    k: &str,
) -> Result<&'a serde_json::Map<String, serde_json::Value>, String> {
    obj.get(k)
        .and_then(|v| v.as_object())
        .ok_or_else(|| format!("M4_EXPECT_OBJECT {}", k))
}

fn expect_arr<'a>(
    obj: &'a serde_json::Map<String, serde_json::Value>,
    k: &str,
) -> Result<&'a Vec<serde_json::Value>, String> {
    obj.get(k)
        .and_then(|v| v.as_array())
        .ok_or_else(|| format!("M4_EXPECT_ARRAY {}", k))
}

pub fn parse_manifest_bytes(
    bytes: &[u8],
) -> Result<(String, BTreeMap<String, Vec<String>>), String> {
    let v: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|_| "M4_MANIFEST_PARSE_FAIL".to_string())?;
    let root = v
        .as_object()
        .ok_or_else(|| "M4_MANIFEST_NOT_OBJECT".to_string())?;
    expect_only_keys(root, &["modules", "v"])?;
    let ver = root
        .get("v")
        .and_then(|x| x.as_str())
        .ok_or_else(|| "M4_MANIFEST_MISSING_v".to_string())?;
    if ver != "0.5-m4" {
        return Err("M4_MANIFEST_BAD_v".into());
    }

    let modules = expect_obj(root, "modules")?;

    let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (mname, mv) in modules.iter() {
        let mo = mv
            .as_object()
            .ok_or_else(|| "M4_MODULE_NOT_OBJECT".to_string())?;
        expect_only_keys(mo, &["exports"])?;
        let ex = expect_arr(mo, "exports")?;
        let mut exports: Vec<String> = Vec::new();
        for e in ex.iter() {
            let s = e
                .as_str()
                .ok_or_else(|| "M4_EXPORT_NOT_STRING".to_string())?;
            exports.push(s.to_string());
        }
        if exports.is_empty() {
            return Err("M4_EXPORTS_EMPTY".into());
        }
        out.insert(mname.to_string(), exports);
    }

    Ok((ver.to_string(), out))
}

pub fn require_minimum_surface(mods: &BTreeMap<String, Vec<String>>) -> Result<(), String> {
    let req = [
        "std/result",
        "std/option",
        "std/list",
        "std/rec",
        "std/str",
        "std/json",
        "std/http",
        "std/fs",
        "std/path",
        "std/null",
        "std/int",
        "std/schema",
        "std/flow",
        "std/grow",
        "std/map",
        "std/time",
        "std/trace",
        "std/artifact",
        "std/hash",
    ];
    for m in req.iter() {
        if !mods.contains_key(*m) {
            return Err(format!("M4_MISSING_MODULE {}", m));
        }
    }
    Ok(())
}

pub fn builtin_std_export_index() -> BTreeMap<String, BTreeSet<String>> {
    let mut m: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();

    fn add(m: &mut BTreeMap<String, BTreeSet<String>>, module: &str, export: &str) {
        m.entry(module.to_string())
            .or_insert_with(BTreeSet::new)
            .insert(export.to_string());
    }

    // NOTE: wire these to your real builtin_std maps when you move this into src/builtin_std.rs.
    // For M4 gates we intentionally read the real runtime export tables via include! in the test,
    // so this placeholder is never used there.
    add(&mut m, "std/result", "ok");
    add(&mut m, "std/result", "err");

    m
}

pub fn assert_builtin_satisfies_manifest(
    manifest: &BTreeMap<String, Vec<String>>,
    builtin: &BTreeMap<String, BTreeSet<String>>,
) -> Result<(), String> {
    for (mname, exports) in manifest.iter() {
        let b = builtin
            .get(mname)
            .ok_or_else(|| format!("M4_BUILTIN_MISSING_MODULE {}", mname))?;
        for e in exports.iter() {
            if !b.contains(e) {
                return Err(format!("M4_BUILTIN_MISSING_EXPORT {}::{}", mname, e));
            }
        }
    }
    Ok(())
}
