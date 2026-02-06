#![allow(dead_code)]

use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy)]
pub struct BuiltinSig {
    pub arity_min: usize,
    pub value_first: bool,
}

/// Runtime truth table keyed by fully-qualified surface name:
///     "<module>::<export>"
///
/// Ontology-facing only:
/// - encodes signature shape for Stage enforcement
/// - does NOT encode runtime semantics
pub fn builtin_sig_table_v1() -> BTreeMap<&'static str, BuiltinSig> {
    let mut m: BTreeMap<&'static str, BuiltinSig> = BTreeMap::new();

    let vf1 = BuiltinSig { arity_min: 1, value_first: true };
    let vf2 = BuiltinSig { arity_min: 2, value_first: true };
    let vf3 = BuiltinSig { arity_min: 3, value_first: true };

    // std/result
    m.insert("std/result::isOk", vf1);
    m.insert("std/result::isErr", vf1);
    m.insert("std/result::map", vf2);
    m.insert("std/result::mapErr", vf2);
    m.insert("std/result::andThen", vf2);
    m.insert("std/result::orElse", vf2);
    m.insert("std/result::unwrapOr", vf2);
    m.insert("std/result::unwrapOrElse", vf2);
    m.insert("std/result::toOption", vf1);
    m.insert("std/result::fromOption", vf2);

    // std/option
    m.insert("std/option::isSome", vf1);
    m.insert("std/option::isNone", vf1);
    m.insert("std/option::map", vf2);
    m.insert("std/option::andThen", vf2);
    m.insert("std/option::unwrapOr", vf2);
    m.insert("std/option::unwrapOrElse", vf2);
    m.insert("std/option::toResult", vf2);
    m.insert("std/option::fromNullable", vf1);
    m.insert("std/option::toNullable", vf1);

    // std/null
    m.insert("std/null::isNull", vf1);
    m.insert("std/null::coalesce", vf2);
    m.insert("std/null::guardNotNull", vf2);

    // std/bool
    m.insert("std/bool::not", vf1);

    // std/int
    m.insert("std/int::abs", vf1);
    m.insert("std/int::clamp", vf3);
    m.insert("std/int::toString", vf1);
    m.insert("std/int::parse", vf1);

    // std/num
    m.insert("std/num::round", vf1);
    m.insert("std/num::floor", vf1);
    m.insert("std/num::ceil", vf1);
    m.insert("std/num::sqrt", vf1);
    m.insert("std/num::log", vf1);
    m.insert("std/num::exp", vf1);

    // std/str
    m.insert("std/str::len", vf1);
    m.insert("std/str::trim", vf1);
    m.insert("std/str::toLower", vf1);
    m.insert("std/str::toUpper", vf1);
    m.insert("std/str::split", vf2);
    m.insert("std/str::join", vf2);
    m.insert("std/str::replace", vf3);
    m.insert("std/str::contains", vf2);
    m.insert("std/str::startsWith", vf2);
    m.insert("std/str::endsWith", vf2);
    m.insert("std/str::slice", vf3);
    m.insert("std/str::padLeft", vf3);
    m.insert("std/str::padRight", vf3);

    // std/list
    m.insert("std/list::len", vf1);
    m.insert("std/list::isEmpty", vf1);
    m.insert("std/list::push", vf2);
    m.insert("std/list::map", vf2);
    m.insert("std/list::filter", vf2);
    m.insert("std/list::flatMap", vf2);
    m.insert("std/list::fold", BuiltinSig { arity_min: 3, value_first: true });
    m.insert("std/list::sum", vf1);
    m.insert("std/list::min", vf1);
    m.insert("std/list::max", vf1);
    m.insert("std/list::take", vf2);
    m.insert("std/list::drop", vf2);
    m.insert("std/list::slice", vf3);
    m.insert("std/list::enumerate", vf1);
    m.insert("std/list::groupBy", vf2);
    m.insert("std/list::sort", vf1);
    m.insert("std/list::sortBy", vf2);
    m.insert("std/list::stableSortBy", vf2);
    m.insert("std/list::unique", vf1);
    m.insert("std/list::uniqueBy", vf2);
    m.insert("std/list::chunk", vf2);

    // std/rec
    m.insert("std/rec::keys", vf1);
    m.insert("std/rec::values", vf1);
    m.insert("std/rec::has", vf2);
    m.insert("std/rec::get", vf2);
    m.insert("std/rec::getOr", BuiltinSig { arity_min: 3, value_first: true });
    m.insert("std/rec::getOrErr", BuiltinSig { arity_min: 3, value_first: true });
    m.insert("std/rec::require", BuiltinSig { arity_min: 3, value_first: true });
    m.insert("std/rec::set", BuiltinSig { arity_min: 3, value_first: true });
    m.insert("std/rec::remove", vf2);
    m.insert("std/rec::select", vf2);
    m.insert("std/rec::rename", vf2);
    m.insert("std/rec::update", vf3);

    // std/json
    m.insert("std/json::decode", vf1);
    m.insert("std/json::encode", vf1);
    m.insert("std/json::parse", vf1);
    m.insert("std/json::stringify", vf1);
    m.insert("std/json::pretty", vf1);
    m.insert("std/json::pathGet", vf2);
    m.insert("std/json::pathSet", vf3);

    // std/csv
    m.insert("std/csv::parse", vf1);
    m.insert("std/csv::encode", vf1);
    m.insert("std/csv::withHeader", vf1);
    m.insert("std/csv::toRecords", vf1);

    // std/bytes
    m.insert("std/bytes::len", vf1);
    m.insert("std/bytes::slice", vf3);
    m.insert("std/bytes::toHex", vf1);
    m.insert("std/bytes::fromHex", vf1);

    // std/hash
    m.insert("std/hash::sha256", vf1);
    m.insert("std/hash::sha256Text", vf1);
    m.insert("std/hash::toHex", vf1);

    // std/path
    m.insert("std/path::normalize", vf1);
    m.insert("std/path::dir", vf1);
    m.insert("std/path::base", vf1);
    m.insert("std/path::ext", vf1);
    m.insert("std/path::isAbs", vf1);

    m.insert("std/path::joinAll", BuiltinSig { arity_min: 2, value_first: true });
    // std/fs
    m.insert("std/fs::read", vf1);
    m.insert("std/fs::open", vf1);
    m.insert("std/fs::create", vf1);
    m.insert("std/fs::close", vf1);
    m.insert("std/fs::readAll", vf1);
    m.insert("std/fs::writeAll", vf2);
    m.insert("std/fs::exists", vf1);
    m.insert("std/fs::listDir", vf1);

    // std/http
    m.insert("std/http::get", vf1);
    m.insert("std/http::post", vf2);
    m.insert("std/http::request", vf1);
    m.insert("std/http::okOr", BuiltinSig { arity_min: 3, value_first: true });

    // std/time
    m.insert("std/time::parse", vf1);
    m.insert("std/time::format", vf1);
    m.insert("std/time::add", vf2);
    m.insert("std/time::sub", vf2);

    // std/trace
    m.insert("std/trace::info", vf1);
    m.insert("std/trace::warn", vf1);
    m.insert("std/trace::error", vf1);

    // std/artifact
    m.insert("std/artifact::import", vf1);

    // std/schema
    m.insert("std/schema::check", vf2);

    m
}
