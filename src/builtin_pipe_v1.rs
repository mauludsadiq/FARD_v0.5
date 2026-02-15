#![allow(dead_code)]

use std::collections::BTreeSet;

/// Policy-only allowlist: which fully-qualified stdlib exports are pipeline-Stage.
///
/// This is intentionally minimal and does NOT restate signature shapes.
/// Signature truth remains in builtin_sig_table_v1.
pub fn stage_allowlist_v1() -> BTreeSet<&'static str> {
    let mut s: BTreeSet<&'static str> = BTreeSet::new();

    // Stage: std/result (value-first)
    s.insert("std/result::map");
    s.insert("std/result::mapErr");
    s.insert("std/result::andThen");
    s.insert("std/result::orElse");
    s.insert("std/result::unwrapOr");
    s.insert("std/result::unwrapOrElse");

    // Stage: std/option (value-first)
    s.insert("std/option::map");
    s.insert("std/option::andThen");
    s.insert("std/option::unwrapOr");
    s.insert("std/option::unwrapOrElse");
    s.insert("std/option::toResult");

    // Stage: std/null
    s.insert("std/null::coalesce");
    s.insert("std/null::guardNotNull");

    // Stage: std/int
    s.insert("std/int::clamp");

    // Stage: std/str
    s.insert("std/str::split");
    s.insert("std/str::join");
    s.insert("std/str::replace");
    s.insert("std/str::contains");
    s.insert("std/str::startsWith");
    s.insert("std/str::endsWith");
    s.insert("std/str::slice");
    s.insert("std/str::padLeft");
    s.insert("std/str::padRight");

    // Stage: std/list
    s.insert("std/list::push");
    s.insert("std/list::map");
    s.insert("std/list::filter");
    s.insert("std/list::flatMap");
    s.insert("std/list::fold");
    s.insert("std/list::take");
    s.insert("std/list::drop");
    s.insert("std/list::slice");
    s.insert("std/list::groupBy");
    s.insert("std/list::sortBy");
    s.insert("std/list::stableSortBy");
    s.insert("std/list::uniqueBy");
    s.insert("std/list::chunk");

    // Stage: std/rec
    s.insert("std/rec::has");
    s.insert("std/rec::get");
    s.insert("std/rec::getOr");
    s.insert("std/rec::getOrErr");
    s.insert("std/rec::set");
    s.insert("std/rec::remove");
    s.insert("std/rec::select");
    s.insert("std/rec::rename");
    s.insert("std/rec::update");

    // Stage: std/json
    s.insert("std/json::pathGet");
    s.insert("std/json::pathSet");

    // Stage: std/fs
    s.insert("std/fs::writeAll");

    // Stage: std/http
    s.insert("std/http::post");
    s.insert("std/http::okOr");

    // Stage: std/time
    s.insert("std/time::add");
    s.insert("std/time::sub");

    // Stage: std/path
    s.insert("std/path::joinAll");

    // Stage: std/schema
    s.insert("std/schema::check");

    // Stage: std/color (CQ0)
    s.insert("std/color::hueDegrees");
    s.insert("std/color::hueKey");
    s.insert("std/color::quantize");
    s.insert("std/color::rgbToUnit");

    // Stage: std/image (CQ0)
    s.insert("std/image::encodePNG");

    s
}
