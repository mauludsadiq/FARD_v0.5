use anyhow::{anyhow, bail, Context, Result};
const QMARK_EXPECT_RESULT: &str = "QMARK_EXPECT_RESULT";
const QMARK_PROPAGATE_ERR: &str = "QMARK_PROPAGATE_ERR";
const RESULT_OK_TAG: &str = "ok";
const RESULT_ERR_TAG: &str = "err";
const RESULT_TAG_KEY: &str = "t";
const RESULT_OK_VAL_KEY: &str = "v";
const RESULT_ERR_VAL_KEY: &str = "e";
const ERROR_PAT_MISMATCH: &str = "ERROR_PAT_MISMATCH";
const ERROR_MATCH_NO_ARM: &str = "ERROR_MATCH_NO_ARM";
use serde_json::{Map, Value as J};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
fn main() -> Result<()> {
let (run, want_version) = fard_v0_5_language_gate::cli::fardrun_cli::Cli::parse_compat();
if want_version {
println!("fard_runtime_version={}", env!("CARGO_PKG_VERSION"));
println!("trace_format_version=0.1.0");
println!("stdlib_root_cid=sha256:dev");
return Ok(());
}
let program = run.program;
let out_dir = run.out;
let lockfile = run.lockfile;
let registry_dir = run.registry;
fs::create_dir_all(&out_dir).ok();
let trace_path = out_dir.join("trace.ndjson");
let result_path = out_dir.join("result.json");
let mut tracer = Tracer::new(&out_dir, &trace_path)?;
let mut loader = ModuleLoader::new(program.parent().unwrap_or(Path::new(".")));
if let Some(rp) = registry_dir.clone() {
loader.registry_dir = Some(rp);
}
if let Some(lockp) = lockfile {
loader.lock = Some(Lockfile::load(&lockp)?);
}
let v = match loader.eval_main(&program, &mut tracer) {
Ok(v) => v,
Err(e) => {
let msg0 = format!("{}", e);
let code = {
const PINNED: &[&str] = &[
QMARK_EXPECT_RESULT,
QMARK_PROPAGATE_ERR,
ERROR_PAT_MISMATCH,
ERROR_MATCH_NO_ARM,
];
msg0.split_whitespace()
.find(|w| PINNED.contains(w))
.or_else(|| {
msg0.split_whitespace()
.find(|w| w.starts_with("ERROR_") && *w != "ERROR_RUNTIME")
})
.or_else(|| msg0.split_whitespace().find(|w| w.starts_with("ERROR_")))
.unwrap_or("ERROR_RUNTIME")
.to_string()
};
let msg = {
let mut s = msg0.clone();
if let Some(rest) = s.strip_prefix("ERROR_RUNTIME ") {
s = rest.to_string();
}
if code != "ERROR_RUNTIME" {
if let Some(rest) = s.strip_prefix(&format!("{} ", code)) {
s = rest.to_string();
}
format!("{} {}", code, s)
} else {
s
}
};
let mut em = Map::new();
  let mut extra_e: Option<J> = None;
  if code == QMARK_PROPAGATE_ERR {
  if let Some(q) = e.downcast_ref::<QMarkUnwind>() {
  if let Some(j) = q.err.to_json() {
  extra_e = Some(j);
  }
  }
  }

em.insert("code".to_string(), J::String(code.clone()));
em.insert("message".to_string(), J::String(msg.clone()));
if let Some(se) = e.downcast_ref::<SpannedRuntimeError>() {
let mut bs = se.span.byte_start;
let mut be = se.span.byte_end;
let mut ln = se.span.line;
let mut cl = se.span.col;
if let Ok(src) = fs::read_to_string(&se.span.file) {
let abs_s = bs.min(src.len());
let ls = src[..abs_s].rfind("\n").map(|i| i + 1).unwrap_or(0);
let rel_s = abs_s.saturating_sub(ls);
let abs_e = be.min(src.len());
let le = src[..abs_e].rfind("\n").map(|i| i + 1).unwrap_or(0);
let rel_e = abs_e.saturating_sub(le);
bs = rel_s;
be = rel_e;
cl = rel_s + 1;
ln = src[..ls].bytes().filter(|b| *b == b"\n"[0]).count() + 1;
}
let mut sm = Map::new();
sm.insert("file".to_string(), J::String(se.span.file.clone()));
sm.insert("byte_start".to_string(), J::Number((bs as u64).into()));
sm.insert("byte_end".to_string(), J::Number((be as u64).into()));
sm.insert("line".to_string(), J::Number((ln as u64).into()));
sm.insert("col".to_string(), J::Number((cl as u64).into()));
em.insert("span".to_string(), J::Object(sm));
} else if let Some(pe) = e.downcast_ref::<ParseError>() {
// Stored spans are absolute offsets; G39 expects line-relative byte offsets.
let mut bs = pe.span.byte_start;
let mut be = pe.span.byte_end;
let mut ln = pe.span.line;
let mut cl = pe.span.col;
if let Ok(src) = fs::read_to_string(&pe.span.file) {
let abs_s = bs.min(src.len());
let ls = src[..abs_s].rfind("\n").map(|i| i + 1).unwrap_or(0);
let rel_s = abs_s.saturating_sub(ls);
let abs_e = be.min(src.len());
let le = src[..abs_e].rfind("\n").map(|i| i + 1).unwrap_or(0);
let rel_e = abs_e.saturating_sub(le);
bs = rel_s;
be = rel_e;
cl = rel_s + 1;
ln = src[..ls].bytes().filter(|b| *b == b"\n"[0]).count() + 1;
}
let mut sm = Map::new();
sm.insert("file".to_string(), J::String(pe.span.file.clone()));
sm.insert("byte_start".to_string(), J::Number((bs as u64).into()));
sm.insert("byte_end".to_string(), J::Number((be as u64).into()));
sm.insert("line".to_string(), J::Number((ln as u64).into()));
sm.insert("col".to_string(), J::Number((cl as u64).into()));
em.insert("span".to_string(), J::Object(sm));
}
fs::write(
out_dir.join("error.json"),
serde_json::to_vec(&J::Object(em))?,
)?;

  if let Some(ev) = &extra_e {
  tracer.error_event_with_e(&code, &msg, ev).ok();
  } else {
  tracer.error_event(&code, &msg).ok();
  }
bail!(msg);
}
};
let j = v.to_json().context("final result must be jsonable")?;
let mut root = Map::new();
root.insert("result".to_string(), j);
fs::write(&result_path, serde_json::to_vec(&J::Object(root))?)?;
Ok(())
}
struct Tracer {
w: fs::File,
out_dir: PathBuf,
}
impl Tracer {
fn new(out_dir: &Path, path: &Path) -> Result<Self> {
fs::create_dir_all(out_dir).ok();
fs::create_dir_all(out_dir.join("artifacts")).ok();
let w = fs::File::create(path)?;
Ok(Self {
w,
out_dir: out_dir.to_path_buf(),
})
}
fn emit(&mut self, v: &J) -> Result<()> {
let mut m = Map::new();
m.insert("t".to_string(), J::String("emit".to_string()));
m.insert("v".to_string(), v.clone());
let line = serde_json::to_string(&J::Object(m))?;
self.w.write_all(line.as_bytes())?;
self.w.write_all(b"\n")?;
Ok(())
}
fn grow_node(&mut self, v: &Val) -> Result<()> {
let j = v.to_json().context("grow_node must be jsonable")?;
let mut m = Map::new();
m.insert("t".to_string(), J::String("grow_node".to_string()));
m.insert("v".to_string(), j);
let line = serde_json::to_string(&J::Object(m))?;
self.w.write_all(line.as_bytes())?;
self.w.write_all(b"\n")?;
Ok(())
}
fn artifact_in(&mut self, path: &str, cid: &str) -> Result<()> {
let mut m = Map::new();
m.insert("t".to_string(), J::String("artifact_in".to_string()));
m.insert("path".to_string(), J::String(path.to_string()));
m.insert("cid".to_string(), J::String(cid.to_string()));
let line = serde_json::to_string(&J::Object(m))?;
self.w.write_all(line.as_bytes())?;
self.w.write_all(b"\n")?;
Ok(())
}
fn artifact_out(&mut self, name: &str, cid: &str, bytes: &[u8]) -> Result<()> {
let outp = self.out_dir.join("artifacts").join(name);
fs::write(&outp, bytes)?;
{
let cid_path = if let Some(ext) = outp.extension().and_then(|e| e.to_str()) {
outp.with_extension(format!("{ext}.cid"))
} else {
outp.with_extension("cid")
};
fs::write(&cid_path, format!("{}\n", cid))?;
}
let mut m = Map::new();
m.insert("t".to_string(), J::String("artifact_out".to_string()));
m.insert("name".to_string(), J::String(name.to_string()));
m.insert("cid".to_string(), J::String(cid.to_string()));
let line = serde_json::to_string(&J::Object(m))?;
self.w.write_all(line.as_bytes())?;
self.w.write_all(b"\n")?;
Ok(())
}
fn module_resolve(&mut self, name: &str, kind: &str, cid: &str) -> Result<()> {
  let mut m = Map::new();
  m.insert("t".to_string(), J::String("module_resolve".to_string()));
  m.insert("name".to_string(), J::String(name.to_string()));
  m.insert("kind".to_string(), J::String(kind.to_string()));
  m.insert("cid".to_string(), J::String(cid.to_string()));
  let line = serde_json::to_string(&J::Object(m))?;
  self.w.write_all(line.as_bytes())?;
  self.w.write_all(b"\n")?;
  Ok(())
}

fn error_event_with_e(&mut self, code: &str, message: &str, e: &J) -> Result<()> {
  let mut m = Map::new();
  m.insert("t".to_string(), J::String("error".to_string()));
  m.insert("code".to_string(), J::String(code.to_string()));
  let mut s = message.to_string();
  if let Some(rest) = s.strip_prefix("ERROR_RUNTIME ") {
  s = rest.to_string();
  }
  if let Some(rest) = s.strip_prefix(&format!("{} ", code)) {
  s = rest.to_string();
  }
  m.insert("message".to_string(), J::String(format!("{} {}", code, s)));
  m.insert("e".to_string(), e.clone());
  let line = serde_json::to_string(&J::Object(m))?;
  self.w.write_all(line.as_bytes())?;
  self.w.write_all(b"\n")?;
  Ok(())
  }

fn error_event(&mut self, code: &str, message: &str) -> Result<()> {
let mut m = Map::new();
m.insert("t".to_string(), J::String("error".to_string()));
m.insert("code".to_string(), J::String(code.to_string()));
let mut s = message.to_string();
if let Some(rest) = s.strip_prefix("ERROR_RUNTIME ") {
s = rest.to_string();
}
if let Some(rest) = s.strip_prefix(&format!("{} ", code)) {
s = rest.to_string();
}
m.insert("message".to_string(), J::String(format!("{} {}", code, s)));
let line = serde_json::to_string(&J::Object(m))?;
self.w.write_all(line.as_bytes())?;
self.w.write_all(b"\n")?;
Ok(())
}
}
#[derive(Clone, Debug)]
enum Tok {
Kw(String),
Ident(String),
Num(i64),
Str(String),
Sym(String),
Eof,
}
#[derive(Clone, Debug)]
#[allow(dead_code)]
struct SpanPos {
byte_start: usize,
byte_end: usize,
line: usize,
col: usize,
}
fn is_ident_start(c: char) -> bool {
c.is_ascii_alphabetic() || c == '_'
}
fn is_ident_cont(c: char) -> bool {
c.is_ascii_alphanumeric() || c == '_'
}
#[derive(Clone, Debug)]
struct ErrorSpan {
file: String,
byte_start: usize,
byte_end: usize,
line: usize,
col: usize,
}
#[derive(Debug)]
struct ParseError {
span: ErrorSpan,
message: String,
}
impl std::fmt::Display for ParseError {
fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
write!(f, "{}", self.message)
}
}
impl std::error::Error for ParseError {}
#[derive(Debug, Clone)]
struct SpannedRuntimeError {
span: ErrorSpan,
message: String,
}
impl std::fmt::Display for SpannedRuntimeError {
fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
write!(f, "{}", self.message)
}
}
impl std::error::Error for SpannedRuntimeError {}
fn line_col_at(src: &str, byte_pos: usize) -> (usize, usize) {
let mut line: usize = 1;
let mut col: usize = 1;
let mut i: usize = 0;
for ch in src.chars() {
if i >= byte_pos {
break;
}
if ch == '\n' {
line += 1;
col = 1;
} else {
col += 1;
}
i += 1;
}
(line, col)
}
struct Lex {
s: Vec<char>,
i: usize,
}
impl Lex {
fn new(src: &str) -> Self {
Self {
s: src.chars().collect(),
i: 0,
}
}
fn peek(&self) -> Option<char> {
self.s.get(self.i).copied()
}
fn bump(&mut self) -> Option<char> {
let c = self.peek()?;
self.i += 1;
Some(c)
}
fn skip_ws(&mut self) {
while let Some(c) = self.peek() {
if c == "#".chars().next().unwrap() {
while let Some(d) = self.bump() {
if d == "\n".chars().next().unwrap() {
break;
}
}
continue;
}
if c.is_whitespace() {
self.i += 1;
continue;
}
if c == '/' && self.s.get(self.i + 1) == Some(&'/') {
while let Some(d) = self.peek() {
self.i += 1;
if d == '\n' {
break;
}
}
continue;
}
break;
}
}
fn next(&mut self) -> Result<Tok> {
self.skip_ws();
let Some(c) = self.peek() else {
return Ok(Tok::Eof);
};
if is_ident_start(c) {
let mut t = String::new();
t.push(self.bump().unwrap());
while let Some(d) = self.peek() {
if is_ident_cont(d) {
t.push(self.bump().unwrap());
} else {
break;
}
}
{
let id = t;
let kws = [
"let", "in", "fn", "if", "then", "else", "import", "as", "export", "match",
"using", "true", "false", "null",
];
if kws.contains(&id.as_str()) {
return Ok(Tok::Kw(id));
}
return Ok(Tok::Ident(id));
}
}
if c.is_ascii_digit() {
let mut n: i64 = 0;
while let Some(d) = self.peek() {
if d.is_ascii_digit() {
n = n * 10 + (d as i64 - '0' as i64);
self.i += 1;
} else {
break;
}
}
return Ok(Tok::Num(n));
}
if c == '"' {
self.bump();
let mut t = String::new();
while let Some(d) = self.bump() {
if d == '"' {
break;
}
if d == '\\' {
let e = self.bump().ok_or_else(|| anyhow!("bad escape"))?;
match e {
'n' => t.push('\n'),
't' => t.push('\t'),
'"' => t.push('"'),
'\\' => t.push('\\'),
_ => bail!("bad escape: \\{e}"),
}
} else {
t.push(d);
}
}
return Ok(Tok::Str(t));
}
let three = if self.i + 2 < self.s.len() {
let mut t = String::new();
t.push(self.s[self.i]);
t.push(self.s[self.i + 1]);
t.push(self.s[self.i + 2]);
Some(t)
} else {
None
};
if three.as_deref() == Some("...") {
self.i += 3;
return Ok(Tok::Sym("...".to_string()));
}
let two = if self.i + 1 < self.s.len() {
let mut t = String::new();
t.push(self.s[self.i]);
t.push(self.s[self.i + 1]);
Some(t)
} else {
None
};
for op in ["==", "<=", ">=", "&&", "||", "->", "=>"] {
if two.as_deref() == Some(op) {
self.i += 2;
return Ok(Tok::Sym(op.to_string()));
}
}
let one = self.bump().unwrap();
let sym = match one {
'(' | ')' | '{' | '}' | '[' | ']' | ',' | ':' | '.' | '+' | '-' | '*' | '/' | '='
| '|' | '<' | '>' | '?' => one.to_string(),
_ => bail!("unexpected char: {one}"),
};
Ok(Tok::Sym(sym))
}
}
#[derive(Clone, Debug, PartialEq, Eq)]
enum Type {
Int,
String,
Bool,
Unit,
List(Box<Type>),
Rec(Vec<(String, Type)>),
Func(Vec<Type>, Box<Type>),
#[allow(dead_code)]
Var(String),
Named(String, Vec<Type>),
Dynamic,
}
#[derive(Clone, Debug)]
#[allow(dead_code)]
enum Pat {
Wild,
Bind(String),
LitInt(i64),
LitStr(String),
LitBool(bool),
LitNull,
Obj {
items: Vec<(String, Pat)>,
rest: Option<String>,
},
List {
items: Vec<Pat>,
rest: Option<String>,
},
}
#[derive(Clone, Debug)]
#[allow(dead_code)]
struct MatchArm {
pat: Pat,
guard: Option<Expr>, // match-arm guard: pat if <expr> => body
guard_span: Option<ErrorSpan>,
body: Expr,
}
#[derive(Clone, Debug)]
#[allow(dead_code)]
enum Expr {
Let(String, Box<Expr>, Box<Expr>),
LetPat(Pat, Box<Expr>, Box<Expr>),
If(Box<Expr>, Box<Expr>, Box<Expr>),
Fn(Vec<Pat>, Box<Expr>),
Lambda(Vec<Pat>, Box<Expr>),
Call(Box<Expr>, Vec<Expr>),
Get(Box<Expr>, String),
List(Vec<Expr>),
Rec(Vec<(String, Expr)>),
Var(String),
Int(i64),
Bool(bool),
Str(String),
Null,
Bin(String, Box<Expr>, Box<Expr>),
Unary(String, Box<Expr>),
Try(Box<Expr>),
Match(Box<Expr>, Vec<MatchArm>),
Using(Pat, Box<Expr>, Box<Expr>),
}
#[derive(Clone, Debug)]
enum Item {
Import(String, String),
Let(String, Expr),
Fn(String, Vec<(Pat, Option<Type>)>, Option<Type>, Expr),
Export(Vec<String>),
Expr(Expr),
}
struct Parser {
toks: Vec<Tok>,
spans: Vec<(usize, usize)>,
file: String,
src: String,
i: usize,
}
impl Parser {
fn from_src(src: &str, file: &str) -> Result<Self> {
let mut lx = Lex::new(src);
let mut toks = Vec::new();
let mut spans: Vec<(usize, usize)> = Vec::new();
loop {
// IMPORTANT: spans must begin at the first token byte, not at preceding whitespace/comments
lx.skip_ws();
let byte_start = lx.i;
let t = lx.next()?;
let byte_end = lx.i;
let done = matches!(t, Tok::Eof);
toks.push(t);
spans.push((byte_start, byte_end));
if done {
break;
}
}
Ok(Self {
toks,
spans,
file: file.to_string(),
src: src.to_string(),
i: 0,
})
}
fn peek(&self) -> &Tok {
self.toks.get(self.i).unwrap_or(&Tok::Eof)
}
fn peek_n(&self, n: usize) -> &Tok {
self.toks.get(self.i + n).unwrap_or(&Tok::Eof)
}
fn bump(&mut self) -> Tok {
let t = self.peek().clone();
self.i += 1;
t
}
fn cur_span(&self) -> ErrorSpan {
// Many parse errors are reported after we already advanced `i`
// (e.g. via bump / expect_*). Use the previous token span when possible.
let idx = if self.i > 0 { self.i - 1 } else { 0 };
let (byte_start, byte_end) = self.spans.get(idx).cloned().unwrap_or((0usize, 0usize));
let (line, col) = line_col_at(&self.src, byte_start);
ErrorSpan {
file: self.file.clone(),
byte_start,
byte_end,
line,
col,
}
}
fn tok_span(&self, idx: usize) -> ErrorSpan {
let (byte_start, byte_end) = self.spans.get(idx).cloned().unwrap_or((0usize, 0usize));
let (line, col) = line_col_at(&self.src, byte_start);
ErrorSpan {
file: self.file.clone(),
byte_start,
byte_end,
line,
col,
}
}
fn span_range(&self, lo_idx: usize, hi_idx: usize) -> ErrorSpan {
let lo = self.tok_span(lo_idx);
let hi = self.tok_span(hi_idx);
ErrorSpan {
file: lo.file,
byte_start: lo.byte_start,
byte_end: hi.byte_end,
line: lo.line,
col: lo.col,
}
}
fn eat_sym(&mut self, s: &str) -> bool {
matches!(self.peek(), Tok::Sym(x) if x == s) && {
self.i += 1;
true
}
}
fn expect_sym(&mut self, s: &str) -> Result<()> {
if self.eat_sym(s) {
Ok(())
} else {
bail!("ERROR_PARSE expected symbol {s:?}")
}
}
fn eat_kw(&mut self, s: &str) -> bool {
if matches!(self.peek(), Tok::Kw(x) if x == s)
|| matches!(self.peek(), Tok::Ident(x) if x == s)
{
self.i += 1;
true
} else {
false
}
}
fn expect_kw(&mut self, s: &str) -> Result<()> {
if self.eat_kw(s) {
Ok(())
} else {
bail!("ERROR_PARSE expected keyword {s}")
}
}
fn expect_ident(&mut self) -> Result<String> {
let t = self.peek().clone();
match self.bump() {
Tok::Ident(x) => Ok(x),
_ => bail!("ERROR_PARSE expected identifier; got {:?}", t),
}
}
fn parse_fn_block_body(&mut self) -> Result<Expr> {
let mut binds: Vec<(String, Expr)> = Vec::new();
while self.eat_kw("let") {
let name = self.expect_ident()?;
self.expect_sym("=")?;
let rhs = self.parse_expr()?;
binds.push((name, rhs));
}
let mut tail = self.parse_expr()?;
self.expect_sym("}")?;
for (name, rhs) in binds.into_iter().rev() {
tail = Expr::Let(name, Box::new(rhs), Box::new(tail));
}
Ok(tail)
}
fn parse_type(&mut self) -> Result<Type> {
match self.peek() {
Tok::Kw(x) | Tok::Ident(x) if x == "Int" => {
self.i += 1;
Ok(Type::Int)
}
Tok::Kw(x) | Tok::Ident(x) if x == "String" => {
self.i += 1;
Ok(Type::String)
}
Tok::Kw(x) | Tok::Ident(x) if x == "Bool" => {
self.i += 1;
Ok(Type::Bool)
}
Tok::Kw(x) | Tok::Ident(x) if x == "Unit" => {
self.i += 1;
Ok(Type::Unit)
}
Tok::Kw(x) | Tok::Ident(x) if x == "Dynamic" => {
self.i += 1;
Ok(Type::Dynamic)
}
Tok::Kw(name) | Tok::Ident(name) => {
let name = name.clone();
self.i += 1;
if name == "List" {
self.expect_sym("<")?;
let inner = self.parse_type()?;
self.expect_sym(">")?;
return Ok(Type::List(Box::new(inner)));
}
if name == "Rec" {
self.expect_sym("{")?;
let mut fields: Vec<(String, Type)> = Vec::new();
if !self.eat_sym("}") {
loop {
let k = self.expect_ident()?;
self.expect_sym(":")?;
let t = self.parse_type()?;
fields.push((k, t));
if self.eat_sym("}") {
break;
}
self.expect_sym(",")?;
}
}
return Ok(Type::Rec(fields));
}
if name == "Func" {
self.expect_sym("(")?;
let mut args: Vec<Type> = Vec::new();
if !self.eat_sym(")") {
loop {
let a = self.parse_type()?;
args.push(a);
if self.eat_sym(")") {
break;
}
self.expect_sym(",")?;
}
}
self.expect_sym("->")?;
let ret = self.parse_type()?;
return Ok(Type::Func(args, Box::new(ret)));
}
if self.eat_sym("<") {
let mut args: Vec<Type> = Vec::new();
if !self.eat_sym(">") {
loop {
let a = self.parse_type()?;
args.push(a);
if self.eat_sym(">") {
break;
}
self.expect_sym(",")?;
}
}
return Ok(Type::Named(name, args));
}
Ok(Type::Named(name, Vec::new()))
}
Tok::Sym(s) if s == "(" => {
self.i += 1;
let t = self.parse_type()?;
self.expect_sym(")")?;
Ok(t)
}
_ => bail!("ERROR_PARSE expected type"),
}
}
#[allow(dead_code)]
fn parse_type_annotation(&mut self) -> Result<Option<Type>> {
if self.eat_sym(":") {
Ok(Some(self.parse_type()?))
} else {
Ok(None)
}
}
fn parse_module(&mut self) -> Result<Vec<Item>> {
let mut items = Vec::new();
while !matches!(self.peek(), Tok::Eof) {
if self.eat_kw("import") {
self.expect_sym("(")?;
let p = match self.bump() {
Tok::Str(s) => s,
_ => bail!("ERROR_PARSE import() requires string"),
};
self.expect_sym(")")?;
self.expect_kw("as")?;
let alias = self.expect_ident()?;
items.push(Item::Import(p, alias));
continue;
}
if self.eat_kw("export") {
self.expect_sym("{")?;
let mut names = Vec::new();
loop {
let n = self.expect_ident()?;
names.push(n);
if self.eat_sym("}") {
break;
}
self.expect_sym(",")?;
if self.eat_sym("}") {
break;
}
}
items.push(Item::Export(names));
continue;
}
if self.eat_kw("fn") {
let name = self.expect_ident()?;
self.expect_sym("(")?;
let mut params: Vec<(Pat, Option<Type>)> = Vec::new();
if !self.eat_sym(")") {
loop {
let p = self.parse_pat()?;
let ann = if self.eat_sym(":") {
Some(self.parse_type()?)
} else {
None
};
params.push((p, ann));
if self.eat_sym(")") {
break;
}
self.expect_sym(",")?;
}
}
let ret: Option<Type> = if self.eat_sym("->") {
Some(self.parse_type()?)
} else {
None
};
self.expect_sym("{")?;
let body = self.parse_fn_block_body()?;
items.push(Item::Fn(name, params, ret, body));
continue;
}
if matches!(self.peek(), Tok::Kw(s) if s == "let") {
let __save = self.i;
if let Ok(e) = self.parse_expr() {
items.push(Item::Expr(e));
continue;
}
self.i = __save;
self.expect_kw("let")?;
let name = self.expect_ident()?;
self.expect_sym("=")?;
let rhs = self.parse_expr()?;
items.push(Item::Let(name, rhs));
continue;
}
let e = self.parse_expr()?;
items.push(Item::Expr(e));
continue;
}
Ok(items)
}
fn parse_pat(&mut self) -> Result<Pat> {
// PAT_LITERALS_V0_5
match self.peek() {
Tok::Num(n) => {
let n = *n;
self.i += 1;
return Ok(Pat::LitInt(n));
}
Tok::Str(s) => {
let s = s.clone();
self.i += 1;
return Ok(Pat::LitStr(s));
}
Tok::Kw(s) if s == "true" => {
self.i += 1;
return Ok(Pat::LitBool(true));
}
Tok::Kw(s) if s == "false" => {
self.i += 1;
return Ok(Pat::LitBool(false));
}
Tok::Kw(s) if s == "null" => {
self.i += 1;
return Ok(Pat::LitNull);
}
_ => {}
}
// literal patterns: Tok::Num/Tok::Str/true/false/null
match self.peek() {
Tok::Num(_) => {
let n = match self.bump() {
Tok::Num(x) => x,
_ => unreachable!(),
};
return Ok(Pat::LitInt(n));
}
Tok::Str(_) => {
let s = match self.bump() {
Tok::Str(x) => x,
_ => unreachable!(),
};
return Ok(Pat::LitStr(s));
}
Tok::Kw(s) | Tok::Ident(s) if s == "true" => {
self.bump();
return Ok(Pat::LitBool(true));
}
Tok::Kw(s) | Tok::Ident(s) if s == "false" => {
self.bump();
return Ok(Pat::LitBool(false));
}
Tok::Kw(s) | Tok::Ident(s) if s == "null" => {
self.bump();
return Ok(Pat::LitNull);
}
_ => {}
}
if self.eat_sym("[") {
let mut items: Vec<Pat> = Vec::new();
let mut rest: Option<String> = None;
if self.eat_sym("]") {
return Ok(Pat::List { items, rest });
}
loop {
if self.eat_sym("...") {
rest = match self.bump() {
Tok::Ident(n) => Some(n),
Tok::Kw(n) => Some(n),
_ => None,
};
self.expect_sym("]")?;
return Ok(Pat::List { items, rest });
}
items.push(self.parse_pat()?);
if self.eat_sym(",") {
if self.eat_sym("]") {
return Ok(Pat::List { items, rest });
}
continue;
}
self.expect_sym("]")?;
return Ok(Pat::List { items, rest });
}
}
if self.eat_sym("{") {
let mut items: Vec<(String, Pat)> = Vec::new();
let mut rest: Option<String> = None;
if self.eat_sym("}") {
return Ok(Pat::Obj { items, rest });
}
loop {
if self.eat_sym("...") {
rest = match self.bump() {
Tok::Ident(n) => Some(n),
Tok::Kw(n) => Some(n),
_ => None,
};
self.expect_sym("}")?;
return Ok(Pat::Obj { items, rest });
}
let k = match self.bump() {
Tok::Ident(x) => x,
Tok::Kw(x) => x,
Tok::Str(x) => x,
_ => bail!("record key must be ident or string"),
};
self.expect_sym(":")?;
let p = self.parse_pat()?;
items.push((k, p));
if self.eat_sym(",") {
if self.eat_sym("}") {
return Ok(Pat::Obj { items, rest });
}
continue;
}
self.expect_sym("}")?;
return Ok(Pat::Obj { items, rest });
}
}
match self.bump() {
Tok::Ident(n) => {
if n == "_" {
Ok(Pat::Wild)
} else {
Ok(Pat::Bind(n))
}
}
Tok::Kw(n) => {
if n == "true" {
Ok(Pat::LitBool(true))
} else if n == "false" {
Ok(Pat::LitBool(false))
} else if n == "null" {
Ok(Pat::LitNull)
} else if n == "_" {
Ok(Pat::Wild)
} else {
Ok(Pat::Bind(n))
}
}
Tok::Num(n) => Ok(Pat::LitInt(n)),
Tok::Str(s) => Ok(Pat::LitStr(s)),
other => bail!("ERROR_PARSE unexpected token: {other:?}"),
}
}
fn parse_match_arms(&mut self) -> Result<Vec<MatchArm>> {
self.expect_sym("{")?;
let mut arms: Vec<MatchArm> = Vec::new();
if self.eat_sym("}") {
return Ok(arms);
}
loop {
let pat = self.parse_pat()?;
let (guard, guard_span) = if self.eat_kw("if") {
let lo_i = self.i;
let g = self.parse_expr()?;
let hi_i = if self.i > 0 { self.i - 1 } else { lo_i };
(Some(g), Some(self.span_range(lo_i, hi_i)))
} else {
(None, None)
};
self.expect_sym("=>")?;
let body = self.parse_expr()?;
arms.push(MatchArm {
pat,
guard,
guard_span,
body,
});
if self.eat_sym(",") {
if self.eat_sym("}") {
break;
}
continue;
}
self.expect_sym("}")?;
break;
}
Ok(arms)
}
fn parse_expr(&mut self) -> Result<Expr> {
if self.eat_kw("using") {
let pat = self.parse_pat()?;
self.expect_sym("=")?;
let acquire = self.parse_expr()?;
self.expect_kw("in")?;
let body = self.parse_expr()?;
return Ok(Expr::Using(pat, Box::new(acquire), Box::new(body)));
}
if self.eat_kw("match") {
let scrut = self.parse_expr()?;
let arms = self.parse_match_arms()?;
return Ok(Expr::Match(Box::new(scrut), arms));
}
if self.eat_kw("let") {
let pat = self.parse_pat()?;
self.expect_sym("=")?;
let e1 = self.parse_expr()?;
self.expect_kw("in")?;
let e2 = self.parse_expr()?;
match &pat {
Pat::Bind(name) => return Ok(Expr::Let(name.clone(), Box::new(e1), Box::new(e2))),
_ => return Ok(Expr::LetPat(pat, Box::new(e1), Box::new(e2))),
}
}
if self.eat_kw("if") {
let c = self.parse_expr()?;
self.expect_kw("then")?;
let t = self.parse_expr()?;
self.expect_kw("else")?;
let f = self.parse_expr()?;
return Ok(Expr::If(Box::new(c), Box::new(t), Box::new(f)));
}
self.parse_or()
}
fn parse_or(&mut self) -> Result<Expr> {
let mut e = self.parse_and()?;
while self.eat_sym("||") {
let r = self.parse_and()?;
e = Expr::Bin("||".to_string(), Box::new(e), Box::new(r));
}
Ok(e)
}
fn parse_and(&mut self) -> Result<Expr> {
let mut e = self.parse_eq()?;
while self.eat_sym("&&") {
let r = self.parse_eq()?;
e = Expr::Bin("&&".to_string(), Box::new(e), Box::new(r));
}
Ok(e)
}
fn parse_eq(&mut self) -> Result<Expr> {
let mut e = self.parse_add()?;
loop {
let op = match self.peek() {
Tok::Sym(x) if x == "==" || x == "<" || x == ">" || x == "<=" || x == ">=" => {
x.clone()
}
_ => break,
};
self.bump();
let r = self.parse_add()?;
e = Expr::Bin(op, Box::new(e), Box::new(r));
}
Ok(e)
}
fn parse_add(&mut self) -> Result<Expr> {
let mut e = self.parse_mul()?;
loop {
if self.eat_sym("+") {
let r = self.parse_mul()?;
e = Expr::Bin("+".to_string(), Box::new(e), Box::new(r));
} else if self.eat_sym("-") {
let r = self.parse_mul()?;
e = Expr::Bin("-".to_string(), Box::new(e), Box::new(r));
} else {
break;
}
}
Ok(e)
}
fn parse_mul(&mut self) -> Result<Expr> {
let mut e = self.parse_unary()?;
loop {
if self.eat_sym("*") {
let r = self.parse_unary()?;
e = Expr::Bin("*".to_string(), Box::new(e), Box::new(r));
} else if self.eat_sym("/") {
let r = self.parse_unary()?;
e = Expr::Bin("/".to_string(), Box::new(e), Box::new(r));
} else {
break;
}
}
Ok(e)
}
fn parse_unary(&mut self) -> Result<Expr> {
if self.eat_sym("-") {
let e = self.parse_unary()?;
return Ok(Expr::Unary("-".to_string(), Box::new(e)));
}
self.parse_pipe()
}
fn parse_pipe(&mut self) -> Result<Expr> {
let mut e = self.parse_postfix()?;
while self.eat_sym("|") {
let rhs = self.parse_postfix()?;
e = match rhs {
Expr::Call(f, mut args) => {
let mut new_args = Vec::new();
new_args.push(e);
new_args.append(&mut args);
Expr::Call(f, new_args)
}
other => Expr::Call(Box::new(other), vec![e]),
};
}
Ok(e)
}
fn parse_postfix(&mut self) -> Result<Expr> {
let mut e = self.parse_primary()?;
loop {
if self.eat_sym("?") {
e = Expr::Try(Box::new(e));
continue;
}
if self.eat_sym(".") {
let n = self.expect_ident()?;
e = Expr::Get(Box::new(e), n);
continue;
}
if self.eat_sym("(") {
let mut args = Vec::new();
if !self.eat_sym(")") {
loop {
let a = self.parse_expr()?;
args.push(a);
if self.eat_sym(")") {
break;
}
self.expect_sym(",")?;
if self.eat_sym(")") {
break;
}
}
}
e = Expr::Call(Box::new(e), args);
continue;
}
break;
}
Ok(e)
}
fn parse_primary(&mut self) -> Result<Expr> {
{
if let Tok::Ident(name) = self.peek().clone() {
if matches!(self.peek_n(1), Tok::Sym(s) if s == "=>") {
self.bump();
self.bump();
let body = self.parse_expr()?;
return Ok(Expr::Lambda(vec![Pat::Bind(name)], Box::new(body)));
}
}
if matches!(self.peek(), Tok::Sym(s) if s == "(") {
let save = self.i;
self.bump();
let mut ps: Vec<Pat> = Vec::new();
if matches!(self.peek(), Tok::Sym(s) if s == ")") {
self.bump();
} else {
loop {
match self.bump() {
Tok::Ident(s) => ps.push(Pat::Bind(s)),
_ => {
self.i = save;
break;
}
}
if matches!(self.peek(), Tok::Sym(s) if s == ",") {
self.bump();
continue;
}
if matches!(self.peek(), Tok::Sym(s) if s == ")") {
self.bump();
break;
}
self.i = save;
break;
}
}
if self.i != save {
if matches!(self.peek(), Tok::Sym(s) if s == "=>") {
self.bump();
let body = self.parse_expr()?;
return Ok(Expr::Lambda(ps, Box::new(body)));
} else {
self.i = save;
}
}
}
}
if self.eat_kw("fn") {
self.expect_sym("(")?;
let mut params = Vec::new();
if !self.eat_sym(")") {
loop {
let p = self.parse_pat()?;
params.push(p);
if self.eat_sym(")") {
break;
}
self.expect_sym(",")?;
if self.eat_sym(")") {
break;
}
}
}
self.expect_sym("{")?;
let body = self.parse_expr()?;
self.expect_sym("}")?;
return Ok(Expr::Fn(params, Box::new(body)));
}
match self.bump() {
Tok::Num(n) => Ok(Expr::Int(n)),
Tok::Str(s) => Ok(Expr::Str(s)),
Tok::Kw(s) if s == "true" => Ok(Expr::Bool(true)),
Tok::Kw(s) if s == "false" => Ok(Expr::Bool(false)),
Tok::Kw(s) if s == "null" => Ok(Expr::Null),
Tok::Ident(s) => Ok(Expr::Var(s)),
Tok::Sym(s) if s == "(" => {
let e = self.parse_expr()?;
self.expect_sym(")")?;
Ok(e)
}
Tok::Sym(s) if s == "[" => {
let mut xs = Vec::new();
if !self.eat_sym("]") {
loop {
xs.push(self.parse_expr()?);
if self.eat_sym("]") {
break;
}
self.expect_sym(",")?;
if self.eat_sym("]") {
break;
}
}
}
Ok(Expr::List(xs))
}
Tok::Sym(s) if s == "{" => {
let mut kvs = Vec::new();
if !self.eat_sym("}") {
loop {
let k = match self.bump() {
Tok::Ident(x) => x,
Tok::Kw(x) => x,
Tok::Str(x) => x,
_ => bail!("record key must be ident or string"),
};
self.expect_sym(":")?;
let v = self.parse_expr()?;
kvs.push((k, v));
if self.eat_sym("}") {
break;
}
self.expect_sym(",")?;
if self.eat_sym("}") {
break;
}
}
}
Ok(Expr::Rec(kvs))
}
other => {
return Err(anyhow!(ParseError {
span: self.cur_span(),
message: format!("ERROR_PARSE unexpected token: {other:?}"),
}));
}
}
}
}
#[derive(Clone, Debug)]
enum Val {
Int(i64),
Bool(bool),
Str(String),
Null,
List(Vec<Val>),
Rec(BTreeMap<String, Val>),
Func(Func),
Builtin(Builtin),
}
#[derive(Debug)]
struct QMarkUnwind {
err: Val,
}
impl std::fmt::Display for QMarkUnwind {
fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
write!(f, "{} {:?}", QMARK_PROPAGATE_ERR, self.err)
}
}
impl std::error::Error for QMarkUnwind {}
#[derive(Clone, Debug)]
struct Func {
params: Vec<Pat>,
body: Expr,
env: Env,
}
#[derive(Clone, Debug)]
enum Builtin {
Unimplemented,
ListMap,
ListFilter,
ListRange,
ListRepeat,
ListConcat,
ListFold,
StrTrim,
StrToLower,
StrSplitLines,
ResultOk,
ResultAndThen,
  ResultUnwrapOk,
  ResultUnwrapErr,
RecEmpty,
RecKeys,
RecValues,
RecHas,
RecGet,
RecGetOr,
RecGetOrErr,
RecSet,
RecRemove,
RecMerge,
RecSelect,
RecRename,
RecUpdate,
ResultErr,
ListGet,
ListSortByIntKey,
GrowUnfoldTree,
GrowAppend,
ImportArtifact,
EmitArtifact,
Emit,
Len,
IntParse,
IntPow,
SortInt,
DedupeSortedInt,
HistInt,
Unfold,
FlowPipe,
FlowId,
FlowTap,
StrLen,
StrConcat,
MapGet,
MapSet,
JsonEncode,
JsonDecode,
}

#[derive(Debug)]
struct QmarkPropagateErr {
    e: Val,
}

impl std::fmt::Display for QmarkPropagateErr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {:?}", QMARK_PROPAGATE_ERR, self.e)
    }
}

impl std::error::Error for QmarkPropagateErr {}

#[derive(Clone, Debug)]
struct Env {
parent: Option<Box<Env>>,
vars: HashMap<String, Val>,
}
impl Env {
fn new() -> Self {
Self {
parent: None,
vars: HashMap::new(),
}
}
fn child(&self) -> Self {
Self {
parent: Some(Box::new(self.clone())),
vars: HashMap::new(),
}
}
fn set(&mut self, k: String, v: Val) {
self.vars.insert(k, v);
}
fn get(&self, k: &str) -> Option<Val> {
if let Some(v) = self.vars.get(k) {
return Some(v.clone());
}
self.parent.as_ref().and_then(|p| p.get(k))
}
}
impl Val {
fn to_json(&self) -> Option<J> {
match self {
Val::Int(n) => Some(J::Number((*n).into())),
Val::Bool(b) => Some(J::Bool(*b)),
Val::Str(s) => Some(J::String(s.clone())),
Val::Null => Some(J::Null),
Val::List(xs) => Some(J::Array(
xs.iter().map(|x| x.to_json()).collect::<Option<Vec<_>>>()?,
)),
Val::Rec(m) => {
let mut obj = Map::new();
for (k, v) in m.iter() {
obj.insert(k.clone(), v.to_json()?);
}
Some(J::Object(obj))
}
Val::Func(_) | Val::Builtin(_) => None,
}
}
}
fn val_from_json(j: &J) -> Result<Val> {
match j {
J::Null => Ok(Val::Null),
J::Bool(b) => Ok(Val::Bool(*b)),
J::Number(n) => {
let i = n
.as_i64()
.ok_or_else(|| anyhow!("ERROR_RUNTIME json number not i64"))?;
Ok(Val::Int(i))
}
J::String(s) => Ok(Val::Str(s.clone())),
J::Array(xs) => {
let mut out = Vec::new();
for x in xs {
out.push(val_from_json(x)?);
}
Ok(Val::List(out))
}
J::Object(m) => {
let mut out = BTreeMap::new();
for (k, v) in m.iter() {
out.insert(k.clone(), val_from_json(v)?);
}
Ok(Val::Rec(out))
}
}
}
fn fard_pat_match_v0_5(p: &Pat, v: &Val, env: &mut Env) -> Result<bool> {
match p {
Pat::Wild => Ok(true),
Pat::Bind(n) => {
env.set(n.clone(), v.clone());
Ok(true)
}
Pat::LitInt(i) => Ok(matches!(v, Val::Int(j) if j == i)),
Pat::LitStr(s) => Ok(matches!(v, Val::Str(t) if t == s)),
Pat::LitBool(b) => Ok(matches!(v, Val::Bool(c) if c == b)),
Pat::LitNull => Ok(matches!(v, Val::Null)),
Pat::List { items, rest } => match v {
Val::List(xs) => {
if xs.len() < items.len() {
return Ok(false);
}
for (i, subp) in items.iter().enumerate() {
if !fard_pat_match_v0_5(subp, &xs[i], env)? {
return Ok(false);
}
}
if let Some(rn) = rest {
let k = items.len();
env.set(rn.clone(), Val::List(xs[k..].to_vec()));
}
Ok(true)
}
_ => Ok(false),
},
Pat::Obj { items, rest } => match v {
Val::Rec(m) => {
for (k, subp) in items.iter() {
let vv = match m.get(k) {
Some(vv) => vv,
None => return Ok(false),
};
if !fard_pat_match_v0_5(subp, vv, env)? {
return Ok(false);
}
}
if let Some(rn) = rest {
let mut rm: BTreeMap<String, Val> = BTreeMap::new();
for (k, vv) in m.iter() {
if items.iter().any(|(kk, _)| kk == k) {
continue;
}
rm.insert(k.clone(), vv.clone());
}
env.set(rn.clone(), Val::Rec(rm));
}
Ok(true)
}
_ => Ok(false),
},
}
}
fn eval(e: &Expr, env: &mut Env, tracer: &mut Tracer, loader: &mut ModuleLoader) -> Result<Val> {
match e {
Expr::Int(n) => Ok(Val::Int(*n)),
Expr::Bool(b) => Ok(Val::Bool(*b)),
Expr::Str(s) => Ok(Val::Str(s.clone())),
Expr::Null => Ok(Val::Null),
Expr::Var(n) => env.get(n).ok_or_else(|| anyhow!("unbound var: {n}")),
Expr::List(xs) => {
let mut out = Vec::new();
for x in xs {
out.push(eval(x, env, tracer, loader)?);
}
Ok(Val::List(out))
}
Expr::Rec(kvs) => {
let mut m = BTreeMap::new();
for (k, v) in kvs {
m.insert(k.clone(), eval(v, env, tracer, loader)?);
}
Ok(Val::Rec(m))
}
Expr::Get(obj, k) => {
let o = eval(obj, env, tracer, loader)?;
match o {
Val::Rec(m) => m
.get(k)
.cloned()
.ok_or_else(|| anyhow!("EXPORT_MISSING missing field {k}")),
_ => bail!("field access on non-record"),
}
}
Expr::Let(name, e1, e2) => {
let v1 = eval(e1, env, tracer, loader)?;
let mut child = env.child();
child.set(name.clone(), v1);
eval(e2, &mut child, tracer, loader)
}
Expr::LetPat(pat, e1, e2) => {
let v1 = eval(e1, env, tracer, loader)?;
let mut child = env.child();
if !fard_pat_match_v0_5(pat, &v1, &mut child)? {
bail!("{} let pattern did not match", ERROR_PAT_MISMATCH);
}
eval(e2, &mut child, tracer, loader)
}
Expr::If(c, t, f) => {
let cv = eval(c, env, tracer, loader)?;
match cv {
Val::Bool(true) => eval(t, env, tracer, loader),
Val::Bool(false) => eval(f, env, tracer, loader),
_ => bail!("if cond must be bool"),
}
}
Expr::Fn(params, body) => Ok(Val::Func(Func {
params: params.clone(),
body: (*body.clone()),
env: env.clone(),
})),
Expr::Lambda(params, body) => Ok(Val::Func(Func {
params: params.clone(),
body: (*body.clone()),
env: env.clone(),
})),
Expr::Unary(op, a) => {
let v = eval(a, env, tracer, loader)?;
match (op.as_str(), v) {
("-", Val::Int(n)) => Ok(Val::Int(-n)),
_ => bail!("bad unary op"),
}
}
Expr::Bin(op, a, b) => {
let x = eval(a, env, tracer, loader)?;
let y = eval(b, env, tracer, loader)?;
match (op.as_str(), x, y) {
("+", Val::Int(l), Val::Int(r)) => Ok(Val::Int(l + r)),
("-", Val::Int(l), Val::Int(r)) => Ok(Val::Int(l - r)),
("*", Val::Int(l), Val::Int(r)) => Ok(Val::Int(l * r)),
("/", Val::Int(l), Val::Int(r)) => Ok(Val::Int(l / r)),
("==", l, r) => Ok(Val::Bool(val_eq(&l, &r))),
("&&", Val::Bool(l), Val::Bool(r)) => Ok(Val::Bool(l && r)),
("||", Val::Bool(l), Val::Bool(r)) => Ok(Val::Bool(l || r)),
("<", Val::Int(l), Val::Int(r)) => Ok(Val::Bool(l < r)),
(">", Val::Int(l), Val::Int(r)) => Ok(Val::Bool(l > r)),
("<=", Val::Int(l), Val::Int(r)) => Ok(Val::Bool(l <= r)),
(">=", Val::Int(l), Val::Int(r)) => Ok(Val::Bool(l >= r)),
_ => bail!("bad binop {op}"),
}
}
Expr::Call(f, args) => {
let fv = eval(f, env, tracer, loader)?;
let mut av = Vec::new();
for a in args {
av.push(eval(a, env, tracer, loader)?);
}
call(fv, av, tracer, loader)
}
Expr::Try(x) => {
let rv = eval(x, env, tracer, loader)?;
if !is_result_val(&rv) {
bail!("{} expected result", QMARK_EXPECT_RESULT);
}
if result_is_ok(&rv)? {
result_unwrap_ok(&rv)
} else {
let e = result_unwrap_err(&rv)?;
return Err(QMarkUnwind { err: e }.into());
}
}
Expr::Match(scrut, _arms) => {
let sv = eval(scrut, env, tracer, loader)?;
for arm in _arms.into_iter() {
let mut env2 = env.child();
if fard_pat_match_v0_5(&arm.pat, &sv, &mut env2)? {
if let Some(g) = &arm.guard {
let gv = eval(g, &mut env2, tracer, loader)?;
match gv {
Val::Bool(true) => {}
Val::Bool(false) => {
continue;
}
_ => {
let sp = arm
.guard_span
.clone()
.expect("guard_span must exist when guard exists");
return Err(anyhow!(SpannedRuntimeError {
span: sp,
message: "ERROR_RUNTIME match guard not bool".to_string(),
}));
}
}
}
return eval(&arm.body, &mut env2, tracer, loader);
}
}
bail!("{} no match", ERROR_MATCH_NO_ARM)
}
Expr::Using(_pat, acquire, body) => {
let av = eval(acquire, env, tracer, loader)?;
let mut env2 = env.child();
if !fard_pat_match_v0_5(_pat, &av, &mut env2)? {
bail!("{} using pattern did not match", ERROR_PAT_MISMATCH)
}
eval(body, &mut env2, tracer, loader)
}
}
}
fn is_result_val(v: &Val) -> bool {
  match v {
    Val::Rec(m) => {
      if m.len() != 2 {
        return false;
      }
      match m.get(RESULT_TAG_KEY) {
        Some(Val::Str(t)) if t == RESULT_OK_TAG => m.contains_key(RESULT_OK_VAL_KEY),
        Some(Val::Str(t)) if t == RESULT_ERR_TAG => m.contains_key(RESULT_ERR_VAL_KEY),
        _ => false,
      }
    }
    _ => false,
  }
}

fn result_is_ok(v: &Val) -> Result<bool> {
  match v {
    Val::Rec(m) => {
      if m.len() != 2 {
        bail!("{} expected result", QMARK_EXPECT_RESULT);
      }
      match m.get(RESULT_TAG_KEY) {
        Some(Val::Str(t)) if t == RESULT_OK_TAG => {
          if !m.contains_key(RESULT_OK_VAL_KEY) {
            bail!("QMARK_EXPECT_RESULT ok missing v");
          }
          Ok(true)
        }
        Some(Val::Str(t)) if t == RESULT_ERR_TAG => {
          if !m.contains_key(RESULT_ERR_VAL_KEY) {
            bail!("QMARK_EXPECT_RESULT err missing e");
          }
          Ok(false)
        }
        _ => bail!("{} expected result tag", QMARK_EXPECT_RESULT),
      }
    }
    _ => bail!("{} expected result", QMARK_EXPECT_RESULT),
  }
}

fn result_unwrap_ok(v: &Val) -> Result<Val> {
  match v {
    Val::Rec(m) => {
      if m.len() != 2 {
        bail!("{} expected result", QMARK_EXPECT_RESULT);
      }
      match m.get(RESULT_TAG_KEY) {
        Some(Val::Str(t)) if t == RESULT_OK_TAG => m
          .get(RESULT_OK_VAL_KEY)
          .cloned()
          .ok_or_else(|| anyhow!("QMARK_EXPECT_RESULT ok missing v")),
        Some(Val::Str(t)) if t == RESULT_ERR_TAG => {
          bail!("QMARK_EXPECT_RESULT tried unwrap ok on err")
        }
        _ => bail!("{} expected result tag", QMARK_EXPECT_RESULT),
      }
    }
    _ => bail!("{} expected result", QMARK_EXPECT_RESULT),
  }
}

fn result_unwrap_err(v: &Val) -> Result<Val> {
  match v {
    Val::Rec(m) => {
      if m.len() != 2 {
        bail!("{} expected result", QMARK_EXPECT_RESULT);
      }
      match m.get(RESULT_TAG_KEY) {
        Some(Val::Str(t)) if t == RESULT_ERR_TAG => m
          .get(RESULT_ERR_VAL_KEY)
          .cloned()
          .ok_or_else(|| anyhow!("QMARK_EXPECT_RESULT err missing e")),
        Some(Val::Str(t)) if t == RESULT_OK_TAG => {
          bail!("QMARK_EXPECT_RESULT tried unwrap err on ok")
        }
        _ => bail!("{} expected result tag", QMARK_EXPECT_RESULT),
      }
    }
    _ => bail!("{} expected result", QMARK_EXPECT_RESULT),
  }
}

fn mk_result_ok(v: Val) -> Val {
  let mut m = BTreeMap::new();
  m.insert(
    RESULT_TAG_KEY.to_string(),
    Val::Str(RESULT_OK_TAG.to_string()),
  );
  m.insert(RESULT_OK_VAL_KEY.to_string(), v);
  Val::Rec(m)
}

fn mk_result_err(e: Val) -> Val {
  let mut m = BTreeMap::new();
  m.insert(
    RESULT_TAG_KEY.to_string(),
    Val::Str(RESULT_ERR_TAG.to_string()),
  );
  m.insert(RESULT_ERR_VAL_KEY.to_string(), e);
  Val::Rec(m)
}
fn val_eq(a: &Val, b: &Val) -> bool {
match (a, b) {
(Val::Int(x), Val::Int(y)) => x == y,
(Val::Bool(x), Val::Bool(y)) => x == y,
(Val::Str(x), Val::Str(y)) => x == y,
(Val::Null, Val::Null) => true,
(Val::List(xs), Val::List(ys)) => {
xs.len() == ys.len() && xs.iter().zip(ys).all(|(x, y)| val_eq(x, y))
}
(Val::Rec(xm), Val::Rec(ym)) => {
xm.len() == ym.len()
&& xm
.iter()
.all(|(k, xv)| ym.get(k).map(|yv| val_eq(xv, yv)).unwrap_or(false))
}
_ => false,
}
}
fn call(f: Val, args: Vec<Val>, tracer: &mut Tracer, loader: &mut ModuleLoader) -> Result<Val> {
match f {
Val::Builtin(b) => call_builtin(b, args, tracer, loader),
Val::Func(fun) => {
if fun.params.len() != args.len() {
bail!("arity mismatch");
}
let mut e = fun.env.child();
for (p, a) in fun.params.iter().zip(args.into_iter()) {
if !fard_pat_match_v0_5(p, &a, &mut e)? {
bail!("{} arg pattern did not match", ERROR_PAT_MISMATCH);
}
}
match eval(&fun.body, &mut e, tracer, loader) {
Ok(v) => Ok(v),
Err(err) => {
if let Some(q) = err.downcast_ref::<QMarkUnwind>() {
Ok(mk_result_err(q.err.clone()))
} else {
Err(err)
}
}
}
}
_ => bail!("call on non-function"),
}
}
fn call_builtin(
b: Builtin,
args: Vec<Val>,
tracer: &mut Tracer,
loader: &mut ModuleLoader,
) -> Result<Val> {
match b {
Builtin::Unimplemented => bail!("ERROR_RUNTIME UNIMPLEMENTED_BUILTIN"),
Builtin::ResultOk => {
if args.len() != 1 {
bail!("ERROR_BADARG result.ok expects 1 arg");
}
Ok(mk_result_ok(args[0].clone()))
}
Builtin::ResultAndThen => {
  if args.len() != 2 {
    bail!("ERROR_BADARG result.andThen expects 2 args");
  }
  let r = args[0].clone();
  let f = args[1].clone();

  if !is_result_val(&r) {
    bail!("{} expected result", QMARK_EXPECT_RESULT);
  }

  if !result_is_ok(&r)? {
    return Ok(r);
  }

  let v = result_unwrap_ok(&r)?;
  let out = call(f, vec![v], tracer, loader)?;

  if !is_result_val(&out) {
    bail!("{} expected result", QMARK_EXPECT_RESULT);
  }

  Ok(out)
}
    Builtin::ResultUnwrapOk => {
      if args.len() != 1 {
        bail!("ERROR_BADARG result.unwrap_ok expects 1 arg");
      }
      let r = args[0].clone();
      let v = result_unwrap_ok(&r)?;
      Ok(v)
    }
    Builtin::ResultUnwrapErr => {
      if args.len() != 1 {
        bail!("ERROR_BADARG result.unwrap_err expects 1 arg");
      }
      let r = args[0].clone();
      let e = result_unwrap_err(&r)?;
      Ok(e)
    }

    Builtin::ResultErr => {
if args.len() != 1 {
bail!("ERROR_BADARG result.err expects 1 arg");
}
Ok(mk_result_err(args[0].clone()))
}
Builtin::FlowPipe => {
if args.len() != 2 {
bail!("ERROR_BADARG flow.pipe expects 2 args");
}
let mut acc = args[0].clone();
let fs = match &args[1] {
Val::List(xs) => xs.clone(),
_ => bail!("ERROR_BADARG flow.pipe arg1 must be list"),
};
for f in fs {
acc = call(f, vec![acc], tracer, loader)?;
}
Ok(acc)
}
Builtin::FlowId => {
if args.len() != 1 {
bail!("ERROR_BADARG flow.id expects 1 arg");
}
Ok(args[0].clone())
}
Builtin::FlowTap => {
if args.len() != 2 {
bail!("ERROR_BADARG flow.tap expects 2 args");
}
let x = args[0].clone();
let f = args[1].clone();
let _ = call(f, vec![x.clone()], tracer, loader)?;
Ok(x)
}
Builtin::GrowAppend => {
if args.len() != 2 {
bail!("ERROR_BADARG grow.append expects 2 args");
}
let mut xs = match &args[0] {
Val::List(v) => v.clone(),
_ => bail!("ERROR_BADARG grow.append arg0 must be list"),
};
xs.push(args[1].clone());
Ok(Val::List(xs))
}
Builtin::StrLen => {
if args.len() != 1 {
bail!("ERROR_RUNTIME arity");
}
match &args[0] {
Val::Str(s) => Ok(Val::Int(s.len() as i64)),
_ => bail!("ERROR_RUNTIME type"),
}
}
Builtin::StrConcat => {
if args.len() != 2 {
bail!("ERROR_RUNTIME arity");
}
let a = match &args[0] {
Val::Str(s) => s,
_ => bail!("ERROR_RUNTIME type"),
};
let b = match &args[1] {
Val::Str(s) => s,
_ => bail!("ERROR_RUNTIME type"),
};
Ok(Val::Str(format!("{}{}", a, b)))
}
Builtin::MapGet => {
if args.len() != 2 {
bail!("ERROR_RUNTIME arity");
}
let m = match &args[0] {
Val::Rec(mm) => mm,
_ => bail!("ERROR_RUNTIME type"),
};
let k = match &args[1] {
Val::Str(s) => s,
_ => bail!("ERROR_RUNTIME type"),
};
Ok(m.get(k).cloned().unwrap_or(Val::Null))
}
Builtin::MapSet => {
if args.len() != 3 {
bail!("ERROR_RUNTIME arity");
}
let m = match &args[0] {
Val::Rec(mm) => mm,
_ => bail!("ERROR_RUNTIME type"),
};
let k = match &args[1] {
Val::Str(s) => s,
_ => bail!("ERROR_RUNTIME type"),
};
let v = args[2].clone();
let mut out = m.clone();
out.insert(k.clone(), v);
Ok(Val::Rec(out))
}
Builtin::JsonEncode => {
if args.len() != 1 {
bail!("ERROR_RUNTIME arity");
}
let j = args[0]
.to_json()
.ok_or_else(|| anyhow!("ERROR_RUNTIME json encode non-jsonable"))?;
Ok(Val::Str(serde_json::to_string(&j)?))
}
Builtin::JsonDecode => {
if args.len() != 1 {
bail!("ERROR_RUNTIME arity");
}
let s = match &args[0] {
Val::Str(ss) => ss,
_ => bail!("ERROR_RUNTIME type"),
};
let j: J = serde_json::from_str(s)?;
val_from_json(&j)
}
Builtin::ListGet => {
if args.len() != 2 {
bail!("ERROR_BADARG list.get expects 2 args");
}
let xs = match &args[0] {
Val::List(v) => v.clone(),
_ => bail!("ERROR_BADARG list.get arg0 must be list"),
};
let i = match &args[1] {
Val::Int(n) => *n,
_ => bail!("ERROR_BADARG list.get arg1 must be int"),
};
if i < 0 {
bail!("ERROR_OOB list index out of bounds");
}
let iu = i as usize;
if iu >= xs.len() {
bail!("ERROR_OOB list index out of bounds");
}
return Ok(xs[iu].clone());
}
Builtin::ListSortByIntKey => {
if args.len() != 2 {
bail!("ERROR_BADARG sort_by_int_key expects 2 args");
}
let xs = match &args[0] {
Val::List(v) => v.clone(),
_ => bail!("ERROR_BADARG sort_by_int_key arg0 must be list"),
};
let mut keyed: Vec<(i64, usize, Val)> = Vec::new();
for (idx, it) in xs.into_iter().enumerate() {
let k = match &it {
Val::Rec(m) => match m.get("k") {
Some(Val::Int(n)) => *n,
_ => bail!("ERROR_BADARG sort_by_int_key expects rec.k int"),
},
_ => bail!("ERROR_BADARG sort_by_int_key expects records"),
};
keyed.push((k, idx, it));
}
keyed.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
let out: Vec<Val> = keyed.into_iter().map(|t| t.2).collect();
return Ok(Val::List(out));
}
Builtin::RecEmpty => {
if args.len() != 0 {
bail!("ERROR_BADARG rec.empty expects 0 args");
}
Ok(Val::Rec(BTreeMap::new()))
}
Builtin::RecKeys => {
if args.len() != 1 {
bail!("ERROR_BADARG rec.keys expects 1 arg");
}
let m = match &args[0] {
Val::Rec(mm) => mm,
_ => bail!("ERROR_BADARG rec.keys arg0 must be record"),
};
let mut out: Vec<Val> = Vec::new();
for k in m.keys() {
out.push(Val::Str(k.clone()));
}
Ok(Val::List(out))
}
Builtin::RecValues => {
if args.len() != 1 {
bail!("ERROR_BADARG rec.values expects 1 arg");
}
let m = match &args[0] {
Val::Rec(mm) => mm,
_ => bail!("ERROR_BADARG rec.values arg0 must be record"),
};
let mut out: Vec<Val> = Vec::new();
for v in m.values() {
out.push(v.clone());
}
Ok(Val::List(out))
}
Builtin::RecHas => {
if args.len() != 2 {
bail!("ERROR_BADARG rec.has expects 2 args");
}
let m = match &args[0] {
Val::Rec(mm) => mm,
_ => bail!("ERROR_BADARG rec.has arg0 must be record"),
};
let k = match &args[1] {
Val::Str(s) => s,
_ => bail!("ERROR_BADARG rec.has arg1 must be string"),
};
Ok(Val::Bool(m.contains_key(k)))
}
Builtin::RecGet => {
if args.len() != 2 {
bail!("ERROR_BADARG rec.get expects 2 args");
}
let m = match &args[0] {
Val::Rec(mm) => mm,
_ => bail!("ERROR_BADARG rec.get arg0 must be record"),
};
let k = match &args[1] {
Val::Str(s) => s,
_ => bail!("ERROR_BADARG rec.get arg1 must be string"),
};
Ok(m.get(k).cloned().unwrap_or(Val::Null))
}
Builtin::RecGetOr => {
if args.len() != 3 {
bail!("ERROR_BADARG rec.getOr expects 3 args");
}
let m = match &args[0] {
Val::Rec(mm) => mm,
_ => bail!("ERROR_BADARG rec.getOr arg0 must be record"),
};
let k = match &args[1] {
Val::Str(s) => s,
_ => bail!("ERROR_BADARG rec.getOr arg1 must be string"),
};
let d = args[2].clone();
Ok(m.get(k).cloned().unwrap_or(d))
}
Builtin::RecGetOrErr => {
if args.len() != 3 {
bail!("ERROR_BADARG rec.getOrErr expects 3 args");
}
let m = match &args[0] {
Val::Rec(mm) => mm,
_ => bail!("ERROR_BADARG rec.getOrErr arg0 must be record"),
};
let k = match &args[1] {
Val::Str(s) => s,
_ => bail!("ERROR_BADARG rec.getOrErr arg1 must be string"),
};
let msg = args[2].clone();
match m.get(k) {
Some(v) => Ok(mk_result_ok(v.clone())),
None => Ok(mk_result_err(msg)),
}
}
Builtin::RecSet => {
if args.len() != 3 {
bail!("ERROR_BADARG rec.set expects 3 args");
}
let m = match &args[0] {
Val::Rec(mm) => mm,
_ => bail!("ERROR_BADARG rec.set arg0 must be record"),
};
let k = match &args[1] {
Val::Str(s) => s,
_ => bail!("ERROR_BADARG rec.set arg1 must be string"),
};
let v = args[2].clone();
let mut out = m.clone();
out.insert(k.clone(), v);
Ok(Val::Rec(out))
}
Builtin::RecRemove => {
if args.len() != 2 {
bail!("ERROR_BADARG rec.remove expects 2 args");
}
let m = match &args[0] {
Val::Rec(mm) => mm,
_ => bail!("ERROR_BADARG rec.remove arg0 must be record"),
};
let k = match &args[1] {
Val::Str(s) => s,
_ => bail!("ERROR_BADARG rec.remove arg1 must be string"),
};
let mut out = m.clone();
out.remove(k);
Ok(Val::Rec(out))
}
Builtin::RecMerge => {
if args.len() != 2 {
bail!("ERROR_BADARG rec.merge expects 2 args");
}
let a = match &args[0] {
Val::Rec(mm) => mm,
_ => bail!("ERROR_BADARG rec.merge arg0 must be record"),
};
let b = match &args[1] {
Val::Rec(mm) => mm,
_ => bail!("ERROR_BADARG rec.merge arg1 must be record"),
};
let mut out = a.clone();
for (k, v) in b.iter() {
out.insert(k.clone(), v.clone());
}
Ok(Val::Rec(out))
}
Builtin::RecSelect => {
if args.len() != 2 {
bail!("ERROR_BADARG rec.select expects 2 args");
}
let m = match &args[0] {
Val::Rec(mm) => mm,
_ => bail!("ERROR_BADARG rec.select arg0 must be record"),
};
let ks = match &args[1] {
Val::List(v) => v,
_ => bail!("ERROR_BADARG rec.select arg1 must be list"),
};
let mut out: BTreeMap<String, Val> = BTreeMap::new();
for x in ks.iter() {
let k = match x {
Val::Str(s) => s,
_ => bail!("ERROR_BADARG rec.select keys must be strings"),
};
if let Some(v) = m.get(k) {
out.insert(k.clone(), v.clone());
}
}
Ok(Val::Rec(out))
}
Builtin::RecRename => {
if args.len() != 3 {
bail!("ERROR_BADARG rec.rename expects 3 args");
}
let m = match &args[0] {
Val::Rec(mm) => mm,
_ => bail!("ERROR_BADARG rec.rename arg0 must be record"),
};
let a = match &args[1] {
Val::Str(s) => s,
_ => bail!("ERROR_BADARG rec.rename arg1 must be string"),
};
let b = match &args[2] {
Val::Str(s) => s,
_ => bail!("ERROR_BADARG rec.rename arg2 must be string"),
};
let mut out = m.clone();
if let Some(v) = out.remove(a) {
out.insert(b.clone(), v);
}
Ok(Val::Rec(out))
}
Builtin::RecUpdate => {
if args.len() != 3 {
bail!("ERROR_BADARG rec.update expects 3 args");
}
let m = match &args[0] {
Val::Rec(mm) => mm,
_ => bail!("ERROR_BADARG rec.update arg0 must be record"),
};
let k = match &args[1] {
Val::Str(s) => s,
_ => bail!("ERROR_BADARG rec.update arg1 must be string"),
};
let f = args[2].clone();
let old = m.get(k).cloned().unwrap_or(Val::Null);
let newv = call(f, vec![old], tracer, loader)?;
let mut out = m.clone();
out.insert(k.clone(), newv);
Ok(Val::Rec(out))
}
Builtin::GrowUnfoldTree => {
if args.len() < 2 {
bail!("ERROR_BADARG unfold_tree expects at least 2 args");
}
let seed = args[0].clone();
let depth = match &args[1] {
Val::Rec(m) => match m.get("depth") {
Some(Val::Int(n)) => *n,
_ => 2,
},
_ => 2,
};
let mut q: std::collections::VecDeque<(Val, i64)> = std::collections::VecDeque::new();
q.push_back((seed.clone(), 0));
while let Some((node, d)) = q.pop_front() {
tracer.grow_node(&node)?;
if d >= depth {
continue;
}
let n = match &node {
Val::Rec(m) => match m.get("n") {
Some(Val::Int(x)) => *x,
_ => 0,
},
_ => 0,
};
let c1 = Val::Rec({
let mut m = BTreeMap::new();
m.insert("n".to_string(), Val::Int(n + 1));
m
});
let c2 = Val::Rec({
let mut m = BTreeMap::new();
m.insert("n".to_string(), Val::Int(n + 2));
m
});
q.push_back((c1, d + 1));
q.push_back((c2, d + 1));
}
return Ok(Val::Null);
}
Builtin::StrTrim => {
if args.len() != 1 {
bail!("ERROR_BADARG str.trim expects 1 arg");
}
let s = match &args[0] {
Val::Str(s) => s.clone(),
_ => bail!("ERROR_BADARG str.trim arg0 must be string"),
};
Ok(Val::Str(s.trim().to_string()))
}
Builtin::StrToLower => {
if args.len() != 1 {
bail!("ERROR_BADARG str.toLower expects 1 arg");
}
let s = match &args[0] {
Val::Str(s) => s.clone(),
_ => bail!("ERROR_BADARG str.toLower arg0 must be string"),
};
Ok(Val::Str(s.to_ascii_lowercase()))
}
Builtin::StrSplitLines => {
if args.len() != 1 {
bail!("ERROR_BADARG str.split_lines expects 1 arg");
}
let s = match &args[0] {
Val::Str(s) => s.clone(),
_ => bail!("ERROR_BADARG str.split_lines arg0 must be string"),
};
            // .lines() drops trailing empty line and handles \r\n
let parts: Vec<Val> = s.lines().map(|x| Val::Str(x.to_string())).collect();
Ok(Val::List(parts))
}
Builtin::ListMap => {
if args.len() != 2 {
bail!("ERROR_BADARG list.map expects 2 args");
}
let xs = match &args[0] {
Val::List(v) => v.clone(),
_ => bail!("ERROR_BADARG list.map arg0 must be list"),
};
let f = args[1].clone();
let mut out: Vec<Val> = Vec::with_capacity(xs.len());
for x in xs {
out.push(call(f.clone(), vec![x], tracer, loader)?);
}
Ok(Val::List(out))
}
Builtin::ListFilter => {
if args.len() != 2 {
bail!("ERROR_BADARG list.filter expects 2 args");
}
let xs = match &args[0] {
Val::List(v) => v.clone(),
_ => bail!("ERROR_BADARG list.filter arg0 must be list"),
};
let pred = args[1].clone();
let mut out: Vec<Val> = Vec::new();
for x in xs {
let keep = call(pred.clone(), vec![x.clone()], tracer, loader)?;
match keep {
Val::Bool(true) => out.push(x),
Val::Bool(false) => {}
Val::Int(n) => { if n != 0 { out.push(x) } }
_ => bail!("ERROR_BADARG list.filter predicate must return bool"),
}
}
Ok(Val::List(out))
}
Builtin::ListRange => {
if args.len() != 1 && args.len() != 2 {
bail!("ERROR_BADARG list.range expects 1 or 2 args");
}
let (start, end) = if args.len() == 1 {
(0i64, match &args[0] { Val::Int(n) => *n, _ => bail!("ERROR_BADARG list.range arg0 must be int") })
} else {
let a = match &args[0] { Val::Int(n) => *n, _ => bail!("ERROR_BADARG list.range arg0 must be int") };
let b = match &args[1] { Val::Int(n) => *n, _ => bail!("ERROR_BADARG list.range arg1 must be int") };
(a, b)
};
if end < start {
bail!("ERROR_BADARG list.range requires end >= start");
}
let mut out: Vec<Val> = Vec::new();
let mut i = start;
while i < end {
out.push(Val::Int(i));
i += 1;
}
Ok(Val::List(out))
}
Builtin::ListRepeat => {
if args.len() != 2 {
bail!("ERROR_BADARG list.repeat expects 2 args");
}
let v = args[0].clone();
let n = match &args[1] {
Val::Int(k) => *k,
_ => bail!("ERROR_BADARG list.repeat arg1 must be int"),
};
if n < 0 {
bail!("ERROR_BADARG list.repeat requires n >= 0");
}
let mut out: Vec<Val> = Vec::new();
let mut i = 0i64;
while i < n {
out.push(v.clone());
i += 1;
}
Ok(Val::List(out))
}
Builtin::ListConcat => {
if args.len() != 1 {
bail!("ERROR_BADARG list.concat expects 1 arg");
}
let xss = match &args[0] {
Val::List(v) => v.clone(),
_ => bail!("ERROR_BADARG list.concat arg0 must be list"),
};
let mut out: Vec<Val> = Vec::new();
for xs in xss {
match xs {
Val::List(v) => out.extend(v),
_ => bail!("ERROR_BADARG list.concat expects list[list[_]]"),
}
}
Ok(Val::List(out))
}
Builtin::ListFold => {
if args.len() != 3 {
bail!("ERROR_BADARG list.fold expects 3 args");
}
let xs = match &args[0] {
Val::List(v) => v.clone(),
_ => bail!("ERROR_BADARG list.fold arg0 must be list"),
};
let mut acc = args[1].clone();
let f = args[2].clone();
for x in xs {
acc = call(f.clone(), vec![acc, x], tracer, loader)?;
}
Ok(acc)
}
Builtin::ImportArtifact => {
if args.len() != 1 {
bail!("ERROR_BADARG import_artifact expects 1 arg");
}
let p = match &args[0] {
Val::Str(s) => s.clone(),
_ => bail!("ERROR_BADARG import_artifact arg must be string"),
};
let disk = tracer.out_dir.join("artifacts").join(&p);
let bytes = match fs::read(&disk) {
Ok(b) => b,
Err(e) => {
return Ok(mk_result_err(Val::Str(format!(
"ERROR_IO cannot read artifact: {p} ({e})"
))));
}
};
let cid = sha256_bytes(&bytes);
tracer.artifact_in(&p, &cid)?;
let text = match String::from_utf8(bytes) {
Ok(s) => s,
Err(e) => {
return Ok(mk_result_err(Val::Str(format!(
"ERROR_UTF8 invalid utf8 in artifact: {p} ({e})"
))));
}
};
let mut rec = std::collections::BTreeMap::new();
rec.insert("text".to_string(), Val::Str(text));
rec.insert("cid".to_string(), Val::Str(cid));
Ok(mk_result_ok(Val::Rec(rec)))
}
Builtin::EmitArtifact => {
if args.len() != 2 {
bail!("ERROR_BADARG emit_artifact expects 2 args");
}
let name = match &args[0] {
Val::Str(s) => s.clone(),
_ => bail!("ERROR_BADARG emit_artifact name must be string"),
};
// Accept:
//  - list[int] bytes
//  - {text: string} (gate convenience)
let bytes: Vec<u8> = match &args[1] {
Val::List(vs) => {
let mut out: Vec<u8> = Vec::with_capacity(vs.len());
for v in vs {
let n = match v {
Val::Int(i) => *i,
_ => {
return Ok(mk_result_err(Val::Str(
"ERROR_BADARG emit_artifact bytes must be ints".to_string(),
)));
}
};
if n < 0 || n > 255 {
return Ok(mk_result_err(Val::Str(
"ERROR_BADARG emit_artifact byte out of range".to_string(),
)));
}
out.push(n as u8);
}
out
}
Val::Rec(m) => match m.get("text") {
Some(Val::Str(s)) => s.as_bytes().to_vec(),
_ => {
return Ok(mk_result_err(Val::Str(
"ERROR_BADARG emit_artifact expects bytes:list[int] or {text:string}"
.to_string(),
)));
}
},
_ => {
return Ok(mk_result_err(Val::Str(
"ERROR_BADARG emit_artifact expects bytes:list[int] or {text:string}"
.to_string(),
)));
}
};
let cid = sha256_bytes(&bytes);
// tracer.artifact_out writes to out_dir/artifacts/<name> and traces it
if let Err(e) = tracer.artifact_out(&name, &cid, &bytes) {
return Ok(mk_result_err(Val::Str(format!(
"ERROR_IO cannot write artifact: {name} ({e})"
))));
}
let mut rec = std::collections::BTreeMap::new();
rec.insert("name".to_string(), Val::Str(name));
rec.insert("cid".to_string(), Val::Str(cid));
Ok(mk_result_ok(Val::Rec(rec)))
}
Builtin::Emit => {
if args.len() != 1 {
bail!("emit arity");
}
let j = args[0]
.to_json()
.ok_or_else(|| anyhow!("emit arg must be jsonable"))?;
tracer.emit(&j)?;
Ok(Val::Null)
}
Builtin::Len => {
if args.len() != 1 {
bail!("len arity");
}
match &args[0] {
Val::List(xs) => Ok(Val::Int(xs.len() as i64)),
Val::Str(s) => Ok(Val::Int(s.as_bytes().len() as i64)),
_ => bail!("len expects list or string"),
}
}
Builtin::IntParse => {
    if args.len() != 1 {
        bail!("ERROR_BADARG int.parse expects 1 arg");
    }
    let s = match &args[0] {
        Val::Str(s) => s.clone(),
        _ => bail!("ERROR_BADARG int.parse arg0 must be string"),
    };
    match s.trim().parse::<i64>() {
        Ok(n) => Ok(mk_result_ok(Val::Int(n))),
        Err(e) => Ok(mk_result_err(Val::Str(format!("ERROR_PARSE int.parse ({e})")))),
    }
}


Builtin::IntPow => {

  if args.len() != 2 {

    bail!("ERROR_BADARG int.pow expects 2 args");

  }

  let base = match &args[0] {

    Val::Int(n) => *n,

    _ => bail!("ERROR_BADARG int.pow arg0 must be int"),

  };

  let exp = match &args[1] {

    Val::Int(n) => *n,

    _ => bail!("ERROR_BADARG int.pow arg1 must be int"),

  };

  if exp < 0 {

    bail!("ERROR_BADARG int.pow requires exp >= 0");

  }

  let e: u32 = match u32::try_from(exp) {

    Ok(x) => x,

    Err(_) => bail!("ERROR_BADARG int.pow exp too large"),

  };

  let mut acc: i128 = 1;

  let mut i: u32 = 0;

  while i < e {

    acc = match acc.checked_mul(base as i128) {

      Some(v) => v,

      None => bail!("ERROR_OVERFLOW int.pow overflow"),

    };

    i += 1;

  }

  if acc < (i64::MIN as i128) || acc > (i64::MAX as i128) {

    bail!("ERROR_OVERFLOW int.pow overflow");

  }

  Ok(Val::Int(acc as i64))

}

Builtin::SortInt => {

if args.len() != 1 {
bail!("sort_int arity");
}
let mut xs = match args[0].clone() {
Val::List(v) => v,
_ => bail!("sort_int expects list"),
};
let mut ns: Vec<i64> = Vec::new();
for v in xs.drain(..) {
match v {
Val::Int(n) => ns.push(n),
_ => bail!("sort_int expects ints"),
}
}
insertion_sort(&mut ns);
Ok(Val::List(ns.into_iter().map(Val::Int).collect()))
}
Builtin::DedupeSortedInt => {
if args.len() != 1 {
bail!("dedupe_sorted_int arity");
}
let xs = match args[0].clone() {
Val::List(v) => v,
_ => bail!("dedupe_sorted_int expects list"),
};
let mut out: Vec<i64> = Vec::new();
let mut last: Option<i64> = None;
for v in xs {
let n = match v {
Val::Int(n) => n,
_ => bail!("dedupe_sorted_int expects ints"),
};
if last.map(|x| x == n).unwrap_or(false) {
continue;
}
last = Some(n);
out.push(n);
}
Ok(Val::List(out.into_iter().map(Val::Int).collect()))
}
Builtin::HistInt => {
if args.len() != 1 {
bail!("hist_int arity");
}
let xs = match args[0].clone() {
Val::List(v) => v,
_ => bail!("hist_int expects list"),
};
let mut m: BTreeMap<i64, i64> = BTreeMap::new();
for v in xs {
let n = match v {
Val::Int(n) => n,
_ => bail!("hist_int expects ints"),
};
*m.entry(n).or_insert(0) += 1;
}
let mut out_list: Vec<Val> = Vec::new();
for (v, c) in m {
let mut rec = BTreeMap::new();
rec.insert("k".to_string(), Val::Int(v));
rec.insert("count".to_string(), Val::Int(c));
out_list.push(Val::Rec(rec));
}
Ok(Val::List(out_list))
}
Builtin::Unfold => {
if args.len() != 3 {
bail!("unfold arity");
}
let mut seed = args[0].clone();
let fuel = match &args[1] {
Val::Int(n) => *n,
Val::Rec(m) => {
if let Some(Val::Int(n)) = m.get("fuel") {
*n
} else if let Some(Val::Int(n)) = m.get("steps") {
*n
} else {
bail!("unfold opts must include fuel:int or steps:int");
}
}
_ => bail!("unfold opts must be record or int"),
};
let step = args[2].clone();
let mut out = Vec::new();
let mut k = 0i64;
while k < fuel {
let r = call(step.clone(), vec![seed.clone()], tracer, loader)?;
match r {
Val::Null => break,
Val::Rec(m) => {
let next_seed = if let Some(v) = m.get("seed").cloned() {
v
} else if let Some(v) = m.get("i").cloned() {
let mut mm = BTreeMap::new();
mm.insert("i".to_string(), v);
Val::Rec(mm)
} else {
bail!("unfold step missing seed/i");
};
let val = m
.get("value")
.or_else(|| m.get("out"))
.cloned()
.ok_or_else(|| anyhow!("unfold step missing value/out"))?;
out.push(val);
seed = next_seed;
}
Val::List(xs) => {
if xs.is_empty() {
break;
}
let m = match &xs[0] {
Val::Rec(m) => m,
_ => bail!("unfold step list must contain record"),
};
let next_seed = if let Some(v) = m.get("seed").cloned() {
v
} else if let Some(v) = m.get("i").cloned() {
let mut mm = BTreeMap::new();
mm.insert("i".to_string(), v);
Val::Rec(mm)
} else {
bail!("unfold step missing seed/i");
};
let val = m
.get("value")
.or_else(|| m.get("out"))
.cloned()
.ok_or_else(|| anyhow!("unfold step missing value/out"))?;
out.push(val);
seed = next_seed;
}
_ => bail!("unfold step must return record, list, or null"),
}
k += 1;
}
Ok(Val::List(out))
}
}
}
fn insertion_sort(xs: &mut [i64]) {
for i in 1..xs.len() {
let key = xs[i];
let mut j = i;
while j > 0 && xs[j - 1] > key {
xs[j] = xs[j - 1];
j -= 1;
}
xs[j] = key;
}
}
struct Lockfile {
modules: HashMap<String, String>,
}
impl Lockfile {
fn load(p: &Path) -> Result<Self> {
let bytes = match fs::read(p) {
Ok(b) => b,
Err(e) => {
// If the program has chdir’d (e.g., into --out), a relative lock path
// like "fard.lock.json" will fail. Retry against the shell’s original PWD.
if e.kind() == std::io::ErrorKind::NotFound && !p.is_absolute() {
if let Ok(pwd) = std::env::var("PWD") {
let alt = std::path::Path::new(&pwd).join(p);
fs::read(&alt).with_context(|| {
format!(
"missing lock file: {} (also tried {})",
p.display(),
alt.display()
)
})?
} else {
return Err(e)
.with_context(|| format!("missing lock file: {}", p.display()));
}
} else {
return Err(e).with_context(|| format!("missing lock file: {}", p.display()));
}
}
};
let v: J = serde_json::from_slice(&bytes)?;
let mut modules: HashMap<String, String> = HashMap::new();
// Accept either:
//  (A) object map:
//      "modules": { "spec": "sha256:..." }
//      "modules": { "spec": { "digest": "sha256:..." } }
//  (B) array of entries (fixture format):
//      "modules": [ { "spec": "...", "digest": "...", "path": "..." }, ... ]
if let Some(ms) = v.get("modules").and_then(|x| x.as_object()) {
for (k, vv) in ms {
let dig = if let Some(s) = vv.as_str() {
s.to_string()
} else {
vv.get("digest")
.and_then(|x| x.as_str())
.ok_or_else(|| {
anyhow!(
"ERROR_LOCK modules values must be string. expected string or object with digest key"
)
})?
.to_string()
};
if dig.is_empty() {
bail!("ERROR_LOCK modules digest empty");
}
modules.insert(k.clone(), dig);
}
} else if let Some(arr) = v.get("modules").and_then(|x| x.as_array()) {
for it in arr {
let spec = it
.get("spec")
.and_then(|x| x.as_str())
.ok_or_else(|| anyhow!("ERROR_LOCK modules array entry missing spec"))?;
let dig = it
.get("digest")
.and_then(|x| x.as_str())
.ok_or_else(|| anyhow!("ERROR_LOCK modules array entry missing digest"))?;
if dig.is_empty() {
bail!("ERROR_LOCK modules digest empty");
}
modules.insert(spec.to_string(), dig.to_string());
}
}
Ok(Self { modules })
}
fn expected(&self, k: &str) -> Option<&str> {
self.modules.get(k).map(|s| s.as_str())
}
}
#[derive(Clone, Debug)]
enum ModKind {
Std,
Pkg,
Rel,
}
#[derive(Clone, Debug)]
struct ModNode {
id: usize,
spec: String,
kind: ModKind,
path: Option<String>,
digest: Option<String>,
}
#[derive(Clone, Debug)]
struct ModEdge {
from: usize,
to: usize,
kind: String,
}
#[derive(Clone, Debug)]
struct ModuleGraph {
nodes: Vec<ModNode>,
edges: Vec<ModEdge>,
index: HashMap<String, usize>,
}
impl ModuleGraph {
fn new() -> Self {
Self {
nodes: Vec::new(),
edges: Vec::new(),
index: HashMap::new(),
}
}
fn intern_node(
&mut self,
spec: &str,
kind: ModKind,
path: Option<String>,
digest: Option<String>,
) -> usize {
if let Some(id) = self.index.get(spec) {
let i = *id;
if self.nodes[i].path.is_none() {
self.nodes[i].path = path;
}
if self.nodes[i].digest.is_none() {
self.nodes[i].digest = digest;
}
return i;
}
let id = self.nodes.len();
self.index.insert(spec.to_string(), id);
self.nodes.push(ModNode {
id,
spec: spec.to_string(),
kind,
path,
digest,
});
id
}
fn add_edge(&mut self, from: usize, to: usize) {
self.edges.push(ModEdge {
from,
to,
kind: "import".to_string(),
});
}
fn to_json(&self) -> J {
let mut root = Map::new();
let mut ns: Vec<J> = Vec::new();
for n in &self.nodes {
let mut m = Map::new();
m.insert("id".to_string(), J::Number((n.id as u64).into()));
m.insert("spec".to_string(), J::String(n.spec.clone()));
m.insert(
"kind".to_string(),
J::String(
match n.kind {
ModKind::Std => "std",
ModKind::Pkg => "pkg",
ModKind::Rel => "rel",
}
.to_string(),
),
);
if let Some(p) = &n.path {
m.insert("path".to_string(), J::String(p.clone()));
}
if let Some(d) = &n.digest {
m.insert("digest".to_string(), J::String(d.clone()));
}
ns.push(J::Object(m));
}
let mut es: Vec<J> = Vec::new();
for e in &self.edges {
let mut m = Map::new();
m.insert("from".to_string(), J::Number((e.from as u64).into()));
m.insert("to".to_string(), J::Number((e.to as u64).into()));
m.insert("kind".to_string(), J::String(e.kind.clone()));
es.push(J::Object(m));
}
root.insert("nodes".to_string(), J::Array(ns));
root.insert("edges".to_string(), J::Array(es));
J::Object(root)
}
}
struct ModuleLoader {
root_dir: PathBuf,
registry_dir: Option<PathBuf>,
cache: HashMap<String, BTreeMap<String, Val>>,
stack: Vec<String>,
lock: Option<Lockfile>,
graph: ModuleGraph,
current: Option<usize>,
}
impl ModuleLoader {
fn new(root: &Path) -> Self {
Self {
root_dir: root.to_path_buf(),
registry_dir: None,
cache: HashMap::new(),
stack: Vec::new(),
lock: None,
graph: ModuleGraph::new(),
current: None,
}
}
fn graph_note_import(
&mut self,
callee_spec: &str,
callee_kind: ModKind,
callee_path: Option<String>,
callee_digest: Option<String>,
) -> usize {
let callee_id =
self.graph
.intern_node(callee_spec, callee_kind, callee_path, callee_digest);
if let Some(from) = self.current {
self.graph.add_edge(from, callee_id);
}
callee_id
}
fn with_current<T>(&mut self, id: usize, f: impl FnOnce(&mut Self) -> Result<T>) -> Result<T> {
let prev = self.current;
self.current = Some(id);
let out = f(self);
self.current = prev;
out
}
fn eval_main(&mut self, main_path: &Path, tracer: &mut Tracer) -> Result<Val> {
let src = fs::read_to_string(main_path)
.with_context(|| format!("missing main program file: {}", main_path.display()))?;
let file = main_path.to_string_lossy().to_string();
let mut p = Parser::from_src(&src, &file)?;
let items = p.parse_module()?;
let mut env = base_env();
let here_dir = main_path
.parent()
.map(|p| p.to_path_buf())
.unwrap_or_else(|| self.root_dir.clone());
let main_spec = file.clone();
let main_id =
self.graph
.intern_node(&main_spec, ModKind::Rel, Some(main_spec.clone()), None);
let v = self.with_current(main_id, |slf| {
slf.eval_items(items, &mut env, tracer, &here_dir)
})?;
fs::write(
tracer.out_dir.join("module_graph.json"),
serde_json::to_vec(&self.graph.to_json())?,
)?;
Ok(v)
}
fn eval_items(
&mut self,
items: Vec<Item>,
env: &mut Env,
tracer: &mut Tracer,
here: &Path,
) -> Result<Val> {
let mut exports: Option<Vec<String>> = None;
let mut last: Val = Val::Null;
for it in items {
match it {
Item::Import(path, alias) => {
let ex = self.load_module(&path, here, tracer)?;
env.set(alias, Val::Rec(ex));
}
Item::Let(name, rhs) => {
let v = eval(&rhs, env, tracer, self)?;
env.set(name, v);
}
Item::Fn(name, params, _ret, body) => {
let f = Val::Func(Func {
params: params.into_iter().map(|(p, _)| p).collect(),
body,
env: env.clone(),
});
env.set(name, f);
}
Item::Export(ns) => exports = Some(ns),
Item::Expr(e) => last = eval(&e, env, tracer, self)?,
}
}
if let Some(ns) = exports {
let mut out = BTreeMap::new();
for n in ns {
let v = env
.get(&n)
.ok_or_else(|| anyhow!("export missing name: {n}"))?;
out.insert(n, v);
}
return Ok(Val::Rec(out));
}
Ok(last)
}
fn load_module(
&mut self,
name: &str,
here: &Path,
tracer: &mut Tracer,
) -> Result<BTreeMap<String, Val>> {
if let Some(c) = self.cache.get(name) {
return Ok(c.clone());
}
if self.stack.contains(&name.to_string()) {
bail!("IMPORT_CYCLE cycle detected in imports at {name}");
}
self.stack.push(name.to_string());
let (kind, digest0) = if name.starts_with("std/") {
(ModKind::Std, Some(self.builtin_digest(name)))
} else if name.starts_with("pkg:") || name.starts_with("pkg/") {
(ModKind::Pkg, None)
} else {
(ModKind::Rel, None)
};
let callee_id = self.graph_note_import(name, kind, None, digest0);
let exports = self.with_current(callee_id, |slf| {
let exports = if name.starts_with("std/") {
let ex = slf.builtin_std(name)?;
slf.check_lock(name, &slf.builtin_digest(name))?;
  tracer.module_resolve(name, "std", &slf.builtin_digest(name))?;
ex
} else if name.starts_with("pkg:") || name.starts_with("pkg/") {
if slf.lock.is_none() {
eprintln!("IMPORT_PKG_REQUIRES_LOCK");
bail!("ERROR_LOCK missing lock for pkg import: {name}");
}
let reg = slf
.registry_dir
.as_ref()
.ok_or_else(|| anyhow!("ERROR_REGISTRY missing --registry"))?;
let spec = if let Some(s) = name.strip_prefix("pkg:") {
s
} else if let Some(s) = name.strip_prefix("pkg/") {
s
} else {
name
};
let (pkg, rest) = spec
.split_once("@")
.ok_or_else(|| anyhow!("ERROR_RUNTIME bad pkg import: {name}"))?;
let (ver, mod_id) = rest
.split_once("/")
.ok_or_else(|| anyhow!("ERROR_RUNTIME bad pkg import: {name}"))?;
let base = reg.join("pkgs").join(pkg).join(ver);
let pkg_json_path = base.join("package.json");
let rel: String = if let Ok(pkg_json_bytes) = fs::read(&pkg_json_path) {
let pkg_json: J = serde_json::from_slice(&pkg_json_bytes)
.with_context(|| format!("bad json: {}", pkg_json_path.display()))?;
let entrypoints = pkg_json
.get("entrypoints")
.and_then(|x| x.as_object())
.ok_or_else(|| anyhow!("ERROR_RUNTIME package.json missing entrypoints"))?;
entrypoints
.get(mod_id)
.and_then(|x| x.as_str())
.ok_or_else(|| {
anyhow!("ERROR_RUNTIME missing entrypoint {mod_id} in package.json")
})?
.to_string()
} else {
format!("{mod_id}.fard")
};
let path = base.join("files").join(&rel);
let src = fs::read_to_string(&path)
.with_context(|| format!("missing module file: {}", path.display()))?;
slf.check_lock(name, &file_digest(&path)?)?;
  tracer.module_resolve(name, "pkg", &file_digest(&path)?)?;
let file = path.to_string_lossy().to_string();
let mut p = Parser::from_src(&src, &file)?;
let items = p.parse_module()?;
let mut env = base_env();
let v = slf.eval_items(items, &mut env, tracer, path.parent().unwrap_or(here))?;
match v {
Val::Rec(m) => m,
_ => bail!("module must export a record"),
}
} else if name.starts_with("registry/") {
let reg = slf
.registry_dir
.as_ref()
.ok_or_else(|| anyhow!("ERROR_REGISTRY missing --registry"))?;
let rest = name.strip_prefix("registry/").unwrap_or(name);
let path = reg.join(format!("{rest}.fard"));
let src = fs::read_to_string(&path).with_context(|| {
if path.to_string_lossy().contains("/pkg/") {
eprintln!("IMPORT_PKG_REQUIRES_LOCK");
}
format!("missing module file: {}", path.display())
})?;
slf.check_lock(name, &file_digest(&path)?)?;
  tracer.module_resolve(name, "registry", &file_digest(&path)?)?;
let file = path.to_string_lossy().to_string();
let mut p = Parser::from_src(&src, &file)?;
let items = p.parse_module()?;
let mut env = base_env();
let v = slf.eval_items(items, &mut env, tracer, path.parent().unwrap_or(here))?;
match v {
Val::Rec(m) => m,
_ => bail!("module must export a record"),
}
} else {
let base: &Path = if name.starts_with("lib/") {
slf.root_dir.as_path()
} else {
here
};
let path = base.join(format!("{name}.fard"));
let src = fs::read_to_string(&path).with_context(|| {
if path.to_string_lossy().contains("/pkg/") {
eprintln!("IMPORT_PKG_REQUIRES_LOCK");
}
format!("missing module file: {}", path.display())
})?;
slf.check_lock(name, &file_digest(&path)?)?;
  tracer.module_resolve(name, "rel", &file_digest(&path)?)?;
let file = path.to_string_lossy().to_string();
let mut p = Parser::from_src(&src, &file)?;
let items = p.parse_module()?;
let mut env = base_env();
let v = slf.eval_items(items, &mut env, tracer, path.parent().unwrap_or(here))?;
match v {
Val::Rec(m) => m,
_ => bail!("module must export a record"),
}
};
Ok(exports)
})?;
self.stack.pop();
self.cache.insert(name.to_string(), exports.clone());
Ok(exports)
}
fn check_lock(&self, module: &str, got: &str) -> Result<()> {
if let Some(lk) = &self.lock {
if let Some(exp) = lk.expected(module) {
if exp != got {
bail!("LOCK_MISMATCH lock mismatch for module {module}: expected {exp}, got {got}");
}
}
}
Ok(())
}
fn builtin_std(&self, name: &str) -> Result<BTreeMap<String, Val>> {
match name {
"std/list" => {
let mut m = BTreeMap::new();
m.insert("len".to_string(), Val::Builtin(Builtin::Len));
m.insert("range".to_string(), Val::Builtin(Builtin::ListRange));
m.insert("repeat".to_string(), Val::Builtin(Builtin::ListRepeat));
m.insert("concat".to_string(), Val::Builtin(Builtin::ListConcat));
m.insert("fold".to_string(), Val::Builtin(Builtin::ListFold));
m.insert("map".to_string(), Val::Builtin(Builtin::ListMap));
m.insert("filter".to_string(), Val::Builtin(Builtin::ListFilter));
m.insert("get".to_string(), Val::Builtin(Builtin::ListGet));
m.insert(
"sort_by_int_key".to_string(),
Val::Builtin(Builtin::ListSortByIntKey),
);
m.insert("sort_int".to_string(), Val::Builtin(Builtin::SortInt));
m.insert(
"dedupe_sorted_int".to_string(),
Val::Builtin(Builtin::DedupeSortedInt),
);
m.insert("hist_int".to_string(), Val::Builtin(Builtin::HistInt));
Ok(m)
}
"std/result" => {
let mut m = BTreeMap::new();
m.insert("ok".to_string(), Val::Builtin(Builtin::ResultOk));
m.insert("err".to_string(), Val::Builtin(Builtin::ResultErr));
m.insert("andThen".to_string(), Val::Builtin(Builtin::ResultAndThen));
  m.insert("unwrap_ok".to_string(), Val::Builtin(Builtin::ResultUnwrapOk));
  m.insert("unwrap_err".to_string(), Val::Builtin(Builtin::ResultUnwrapErr));
Ok(m)
}
"std/grow" => {
let mut m = BTreeMap::new();
m.insert("append".to_string(), Val::Builtin(Builtin::GrowAppend));
m.insert("merge".to_string(), Val::Builtin(Builtin::RecMerge));
m.insert(
"unfold_tree".to_string(),
Val::Builtin(Builtin::GrowUnfoldTree),
);
m.insert("unfold".to_string(), Val::Builtin(Builtin::Unfold));
Ok(m)
}
"std/flow" => {
let mut m = BTreeMap::new();
m.insert("id".to_string(), Val::Builtin(Builtin::FlowId));
m.insert("pipe".to_string(), Val::Builtin(Builtin::FlowPipe));
m.insert("tap".to_string(), Val::Builtin(Builtin::FlowTap));
Ok(m)
}
"std/str" => {
let mut m = BTreeMap::new();
m.insert("len".to_string(), Val::Builtin(Builtin::StrLen));
m.insert("trim".to_string(), Val::Builtin(Builtin::StrTrim));
m.insert("split_lines".to_string(), Val::Builtin(Builtin::StrSplitLines));
m.insert("toLower".to_string(), Val::Builtin(Builtin::StrToLower));
m.insert("lower".to_string(), Val::Builtin(Builtin::StrToLower));
m.insert("concat".to_string(), Val::Builtin(Builtin::StrConcat));
Ok(m)
}
"std/map" => {
let mut m = BTreeMap::new();
m.insert("get".to_string(), Val::Builtin(Builtin::MapGet));
m.insert("set".to_string(), Val::Builtin(Builtin::MapSet));
m.insert("keys".to_string(), Val::Builtin(Builtin::RecKeys));
m.insert("values".to_string(), Val::Builtin(Builtin::RecValues));
m.insert("has".to_string(), Val::Builtin(Builtin::RecHas));
Ok(m)
}
"std/rec" => {
let mut m = BTreeMap::new();
m.insert("empty".to_string(), Val::Builtin(Builtin::RecEmpty));
m.insert("keys".to_string(), Val::Builtin(Builtin::RecKeys));
m.insert("values".to_string(), Val::Builtin(Builtin::RecValues));
m.insert("has".to_string(), Val::Builtin(Builtin::RecHas));
m.insert("get".to_string(), Val::Builtin(Builtin::RecGet));
m.insert("getOr".to_string(), Val::Builtin(Builtin::RecGetOr));
m.insert("getOrErr".to_string(), Val::Builtin(Builtin::RecGetOrErr));
m.insert("set".to_string(), Val::Builtin(Builtin::RecSet));
m.insert("remove".to_string(), Val::Builtin(Builtin::RecRemove));
m.insert("merge".to_string(), Val::Builtin(Builtin::RecMerge));
m.insert("select".to_string(), Val::Builtin(Builtin::RecSelect));
m.insert("rename".to_string(), Val::Builtin(Builtin::RecRename));
m.insert("update".to_string(), Val::Builtin(Builtin::RecUpdate));
Ok(m)
}
"std/json" => {
let mut m = BTreeMap::new();
m.insert("encode".to_string(), Val::Builtin(Builtin::JsonEncode));
m.insert("decode".to_string(), Val::Builtin(Builtin::JsonDecode));
Ok(m)
}
"std/int" => {
let mut m = BTreeMap::new();
m.insert("parse".to_string(), Val::Builtin(Builtin::IntParse));
m.insert("pow".to_string(), Val::Builtin(Builtin::IntPow));
Ok(m)
}
"std/option" => {
let mut m = BTreeMap::new();
m.insert("None".to_string(), Val::Builtin(Builtin::Unimplemented));
m.insert("Some".to_string(), Val::Builtin(Builtin::Unimplemented));
m.insert("isNone".to_string(), Val::Builtin(Builtin::Unimplemented));
m.insert("isSome".to_string(), Val::Builtin(Builtin::Unimplemented));
m.insert(
"fromNullable".to_string(),
Val::Builtin(Builtin::Unimplemented),
);
m.insert(
"toNullable".to_string(),
Val::Builtin(Builtin::Unimplemented),
);
m.insert("map".to_string(), Val::Builtin(Builtin::Unimplemented));
m.insert("andThen".to_string(), Val::Builtin(Builtin::Unimplemented));
m.insert("unwrapOr".to_string(), Val::Builtin(Builtin::Unimplemented));
m.insert(
"unwrapOrElse".to_string(),
Val::Builtin(Builtin::Unimplemented),
);
m.insert("toResult".to_string(), Val::Builtin(Builtin::Unimplemented));
Ok(m)
}
"std/null" => {
let mut m = BTreeMap::new();
m.insert("isNull".to_string(), Val::Builtin(Builtin::Unimplemented));
m.insert("coalesce".to_string(), Val::Builtin(Builtin::Unimplemented));
m.insert(
"guardNotNull".to_string(),
Val::Builtin(Builtin::Unimplemented),
);
Ok(m)
}
"std/path" => {
let mut m = BTreeMap::new();
m.insert("base".to_string(), Val::Builtin(Builtin::Unimplemented));
m.insert("dir".to_string(), Val::Builtin(Builtin::Unimplemented));
m.insert("ext".to_string(), Val::Builtin(Builtin::Unimplemented));
m.insert("isAbs".to_string(), Val::Builtin(Builtin::Unimplemented));
m.insert("join".to_string(), Val::Builtin(Builtin::Unimplemented));
m.insert("joinAll".to_string(), Val::Builtin(Builtin::Unimplemented));
m.insert(
"normalize".to_string(),
Val::Builtin(Builtin::Unimplemented),
);
Ok(m)
}
"std/time" => {
let mut m = BTreeMap::new();
m.insert("add".to_string(), Val::Builtin(Builtin::Unimplemented));
m.insert("sub".to_string(), Val::Builtin(Builtin::Unimplemented));
m.insert("format".to_string(), Val::Builtin(Builtin::Unimplemented));
let mut d = BTreeMap::new();
d.insert("ms".to_string(), Val::Builtin(Builtin::Unimplemented));
d.insert("sec".to_string(), Val::Builtin(Builtin::Unimplemented));
d.insert("min".to_string(), Val::Builtin(Builtin::Unimplemented));
m.insert("Duration".to_string(), Val::Rec(d));
Ok(m)
}
"std/trace" => {
let mut m = BTreeMap::new();
m.insert("emit".to_string(), Val::Builtin(Builtin::Unimplemented));
m.insert("info".to_string(), Val::Builtin(Builtin::Unimplemented));
m.insert("warn".to_string(), Val::Builtin(Builtin::Unimplemented));
m.insert("error".to_string(), Val::Builtin(Builtin::Unimplemented));
m.insert("span".to_string(), Val::Builtin(Builtin::Unimplemented));
Ok(m)
}
"std/artifact" => {
let mut m = BTreeMap::new();
m.insert("import".to_string(), Val::Builtin(Builtin::ImportArtifact));
m.insert("emit".to_string(), Val::Builtin(Builtin::EmitArtifact));
m.insert("ref".to_string(), Val::Builtin(Builtin::Unimplemented));
m.insert("derive".to_string(), Val::Builtin(Builtin::Unimplemented));
Ok(m)
}
_ => bail!("unknown std module: {name}"),
}
}
fn builtin_digest(&self, name: &str) -> String {
let mut h = Sha256::new();
h.update(format!("builtin:{name}:v0.5").as_bytes());
format!("sha256:{:x}", h.finalize())
}
}
fn base_env() -> Env {
let mut e = Env::new();
e.set("emit".to_string(), Val::Builtin(Builtin::Emit));
e.set("len".to_string(), Val::Builtin(Builtin::Len));
e.set(
"import_artifact".to_string(),
Val::Builtin(Builtin::ImportArtifact),
);
e.set(
"emit_artifact".to_string(),
Val::Builtin(Builtin::EmitArtifact),
);
e
}
fn sha256_bytes(bytes: &[u8]) -> String {
let mut h = Sha256::new();
h.update(bytes);
format!("sha256:{}", hex::encode(h.finalize()))
}
fn file_digest(p: &Path) -> Result<String> {
let b = fs::read(p)?;
let mut h = Sha256::new();
h.update(&b);
Ok(format!("sha256:{:x}", h.finalize()))
}
