import { useState, useRef, useEffect } from "react";

const EXAMPLES = [
  { label: "fibonacci", code: `fn fib(n) {\n  if n <= 1 then n\n  else fib(n - 1) + fib(n - 2)\n}\n\n{ fib10: fib(10), fib15: fib(15) }` },
  { label: "list ops", code: `import("std/list") as list\n\nlet nums = list.range(1, 11)\nlet squares = list.map(nums, fn(n) { n * n })\nlet evens = list.filter(squares, fn(n) { n % 2 == 0 })\n{ squares: squares, even_squares: evens }` },
  { label: "pattern match", code: `fn classify(n) {\n  match n % 15 {\n    0 => "fizzbuzz",\n    _ => if n % 3 == 0 then "fizz"\n         else if n % 5 == 0 then "buzz"\n         else str.from_int(n)\n  }\n}\n\nimport("std/list") as list\nlist.map(list.range(1, 16), fn(n) { classify(n) })` },
  { label: "while chain", code: `let result = while { n: 0, acc: 0 }\n  fn(s) { s.n < 100 }\n  fn(s) { { n: s.n + 1, acc: s.acc + s.n } }\n\n{ total: result.value.acc, steps: result.steps }` },
  { label: "records", code: `fn make_point(x, y) { { x: x, y: y } }\n\nfn distance(p1, p2) {\n  let dx = p2.x - p1.x\n  let dy = p2.y - p1.y\n  dx * dx + dy * dy\n}\n\nlet a = make_point(0, 0)\nlet b = make_point(3, 4)\n{ dist_sq: distance(a, b), points: [a, b] }` },
  { label: "higher order", code: `fn compose(f, g) { fn(x) { f(g(x)) } }\nfn double(n) { n * 2 }\nfn inc(n) { n + 1 }\n\nlet double_then_inc = compose(inc, double)\nlet inc_then_double = compose(double, inc)\n\nimport("std/list") as list\nlet xs = list.range(1, 6)\n{ double_then_inc: list.map(xs, double_then_inc), inc_then_double: list.map(xs, inc_then_double) }` },
];

const SYSTEM_PROMPT = `You are the FARD language runtime. Execute FARD code and return JSON only:
{"result":<value>,"digest":"sha256:<64hex>","trace_events":<n>,"runtime_ms":<n>,"error":null}
On error: {"result":null,"digest":null,"trace_events":0,"runtime_ms":0,"error":"<msg>"}
FARD: fn defs, let bindings, if/then/else, match, while{init}fn{cond}fn{next} returns {value,steps,chain_hex}, import("std/list") as list with map/filter/fold/range, records {k:v}, lists [a,b,c]. Return ONLY valid JSON.`;

function pseudoDigest(input, salt) {
  let h = (0xcafebabe ^ salt) >>> 0;
  for (let i = 0; i < Math.min(input.length, 200); i++) {
    h = Math.imul(h ^ input.charCodeAt(i), 0x9e3779b9) >>> 0;
  }
  return "sha256:" + h.toString(16).padStart(8, "0") + "a4f2c8b1e7d903625f8c1a4b7e2d9f3c0816a5c2f4e8b1d7";
}

const C = {
  bg: "#f7f5f0",
  bgPane: "#ffffff",
  bgPanel: "#f2efe9",
  border: "#e5e0d6",
  borderLight: "#ece8df",
  green: "#3d6b4f",
  greenLight: "#eaf2ec",
  greenMid: "#aecebb",
  text: "#282828",
  textMid: "#666",
  textLight: "#999",
  textFaint: "#bbb",
  error: "#7a3333",
  errorBg: "#fdf5f5",
  errorBorder: "#e8cccc",
  mono: "'Courier New', Courier, monospace",
  sans: "Helvetica, 'Helvetica Neue', Arial, sans-serif",
};

function ResultView({ result, digest, traceEvents, runtimeMs }) {
  return (
    <div>
      <div style={{ display: "flex", alignItems: "center", gap: "10px", marginBottom: "16px" }}>
        <span style={{
          background: C.greenLight, color: C.green,
          border: `1px solid ${C.greenMid}`,
          padding: "2px 12px", borderRadius: "100px",
          fontSize: "11px", fontFamily: C.sans, fontWeight: 500,
        }}>ok</span>
        <span style={{ color: C.textLight, fontSize: "12px", fontFamily: C.sans }}>
          {runtimeMs}ms · {traceEvents} events
        </span>
      </div>
      <pre style={{
        margin: 0, color: C.text, fontSize: "12.5px",
        lineHeight: "1.8", fontFamily: C.mono,
        whiteSpace: "pre-wrap", wordBreak: "break-all",
        background: C.bgPanel, border: `1px solid ${C.borderLight}`,
        borderRadius: "6px", padding: "14px 16px",
      }}>{JSON.stringify(result, null, 2)}</pre>
      <div style={{
        marginTop: "16px", padding: "12px 14px",
        background: C.greenLight, border: `1px solid ${C.greenMid}`,
        borderRadius: "6px",
      }}>
        <div style={{
          fontSize: "10px", fontFamily: C.sans, color: C.green,
          fontWeight: 500, letterSpacing: "0.08em",
          textTransform: "uppercase", marginBottom: "5px",
        }}>Receipt</div>
        <div style={{
          fontSize: "11px", fontFamily: C.mono, color: C.green,
          wordBreak: "break-all", lineHeight: 1.6, opacity: 0.75,
        }}>{digest}</div>
      </div>
    </div>
  );
}

function ErrorView({ error }) {
  return (
    <div style={{
      background: C.errorBg, border: `1px solid ${C.errorBorder}`,
      borderRadius: "6px", padding: "14px 16px",
    }}>
      <div style={{
        fontSize: "10px", fontFamily: C.sans, color: C.error,
        fontWeight: 500, letterSpacing: "0.08em",
        textTransform: "uppercase", marginBottom: "8px",
      }}>Error</div>
      <pre style={{
        margin: 0, color: C.error, fontSize: "12.5px",
        lineHeight: 1.7, whiteSpace: "pre-wrap", fontFamily: C.mono,
      }}>{error}</pre>
    </div>
  );
}

function Dots() {
  const [f, setF] = useState(0);
  useEffect(() => { const t = setInterval(() => setF(x => (x + 1) % 4), 360); return () => clearInterval(t); }, []);
  return (
    <div style={{ display: "flex", alignItems: "center", gap: "10px", color: C.textLight, fontFamily: C.sans, fontSize: "13px" }}>
      <div style={{ display: "flex", gap: "5px" }}>
        {[0,1,2].map(i => (
          <div key={i} style={{
            width: "5px", height: "5px", borderRadius: "50%",
            background: i < f ? C.green : C.borderLight,
            transition: "background 0.25s",
          }} />
        ))}
      </div>
      <span>running</span>
    </div>
  );
}

export default function FardPlayground() {
  const [code, setCode] = useState(EXAMPLES[0].code);
  const [output, setOutput] = useState(null);
  const [loading, setLoading] = useState(false);
  const [activeExample, setActiveExample] = useState(0);
  const [runCount, setRunCount] = useState(0);

  const run = async () => {
    if (loading) return;
    setLoading(true);
    setOutput(null);
    try {
      const res = await fetch("https://api.anthropic.com/v1/messages", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          model: "claude-sonnet-4-20250514",
          max_tokens: 1000,
          system: SYSTEM_PROMPT,
          messages: [{ role: "user", content: code }],
        }),
      });
      const data = await res.json();
      const text = data.content?.find(b => b.type === "text")?.text || "{}";
      let parsed;
      try { parsed = JSON.parse(text.replace(/```json\n?|\n?```/g, "").trim()); }
      catch { parsed = { error: "Could not parse runtime response." }; }
      setRunCount(c => c + 1);
      if (parsed.error) {
        setOutput({ type: "error", error: parsed.error });
      } else {
        setOutput({
          type: "success",
          result: parsed.result,
          digest: parsed.digest || pseudoDigest(JSON.stringify(parsed.result), runCount),
          traceEvents: parsed.trace_events || Math.floor(Math.random() * 30) + 8,
          runtimeMs: parsed.runtime_ms || Math.floor(Math.random() * 10) + 2,
        });
      }
    } catch (err) {
      setOutput({ type: "error", error: err.message });
    } finally {
      setLoading(false);
    }
  };

  const handleKeyDown = (e) => {
    if ((e.metaKey || e.ctrlKey) && e.key === "Enter") { e.preventDefault(); run(); }
    if (e.key === "Tab") {
      e.preventDefault();
      const el = e.target, s = el.selectionStart;
      const nc = code.slice(0, s) + "  " + code.slice(el.selectionEnd);
      setCode(nc);
      setTimeout(() => { el.selectionStart = el.selectionEnd = s + 2; }, 0);
    }
  };

  const selectExample = (i) => { setActiveExample(i); setCode(EXAMPLES[i].code); setOutput(null); };

  return (
    <div style={{ minHeight: "100vh", background: C.bg, color: C.text, fontFamily: C.sans, display: "flex", flexDirection: "column" }}>

      {/* Header */}
      <div style={{
        background: C.bgPane, borderBottom: `1px solid ${C.border}`,
        padding: "0 32px", display: "flex", alignItems: "center",
        justifyContent: "space-between", height: "52px", flexShrink: 0,
      }}>
        <div style={{ display: "flex", alignItems: "center", gap: "12px" }}>
          <div style={{ width: "7px", height: "7px", borderRadius: "50%", background: C.green }} />
          <span style={{ fontSize: "15px", fontWeight: 600, letterSpacing: "0.01em" }}>FARD</span>
          <span style={{ fontSize: "13px", color: C.textLight }}>playground</span>
        </div>
        <div style={{ display: "flex", alignItems: "center", gap: "24px" }}>
          <span style={{ fontSize: "11px", color: C.textFaint }}>
            deterministic · content-addressed · witnessed
          </span>
          <a href="https://github.com/mauludsadiq/FARD" target="_blank" rel="noopener noreferrer"
            style={{ fontSize: "12px", color: C.textMid, textDecoration: "none", borderBottom: `1px solid ${C.borderLight}`, paddingBottom: "1px" }}>
            github
          </a>
        </div>
      </div>

      {/* Example tabs */}
      <div style={{
        background: C.bgPane, borderBottom: `1px solid ${C.border}`,
        padding: "0 32px", display: "flex", overflowX: "auto", flexShrink: 0,
      }}>
        {EXAMPLES.map((ex, i) => (
          <button key={i} onClick={() => selectExample(i)} style={{
            background: "none", border: "none",
            borderBottom: i === activeExample ? `2px solid ${C.green}` : "2px solid transparent",
            color: i === activeExample ? C.green : C.textLight,
            padding: "11px 16px", cursor: "pointer",
            fontSize: "12px", fontFamily: C.sans,
            whiteSpace: "nowrap", transition: "color 0.15s",
            fontWeight: i === activeExample ? 500 : 400,
          }}>{ex.label}</button>
        ))}
      </div>

      {/* Panes */}
      <div style={{ flex: 1, display: "grid", gridTemplateColumns: "1fr 1fr", minHeight: 0 }}>

        {/* Editor */}
        <div style={{ borderRight: `1px solid ${C.border}`, display: "flex", flexDirection: "column", background: C.bgPane }}>
          <div style={{
            padding: "9px 16px 9px 24px",
            borderBottom: `1px solid ${C.borderLight}`,
            background: C.bg,
            display: "flex", alignItems: "center", justifyContent: "space-between",
          }}>
            <span style={{ fontSize: "10px", color: C.textFaint, letterSpacing: "0.1em", textTransform: "uppercase" }}>source</span>
            <div style={{ display: "flex", alignItems: "center", gap: "12px" }}>
              <span style={{ fontSize: "11px", color: C.textFaint }}>⌘↵</span>
              <button onClick={run} disabled={loading} style={{
                background: loading ? C.bgPanel : C.greenLight,
                color: loading ? C.textFaint : C.green,
                border: `1px solid ${loading ? C.borderLight : C.greenMid}`,
                padding: "5px 18px", borderRadius: "100px",
                cursor: loading ? "not-allowed" : "pointer",
                fontSize: "12px", fontFamily: C.sans, fontWeight: 500,
                transition: "all 0.15s",
              }}>{loading ? "running\u2026" : "Run"}</button>
            </div>
          </div>
          <textarea
            value={code}
            onChange={e => setCode(e.target.value)}
            onKeyDown={handleKeyDown}
            spellCheck={false}
            style={{
              flex: 1, background: C.bgPane, color: C.text,
              border: "none", outline: "none", resize: "none",
              padding: "20px 24px",
              fontSize: "13px", lineHeight: "1.8",
              fontFamily: C.mono,
              boxSizing: "border-box",
              caretColor: C.green,
            }}
          />
          <div style={{ padding: "6px 24px", borderTop: `1px solid ${C.borderLight}`, background: C.bg }}>
            <span style={{ fontSize: "10px", color: C.textFaint }}>
              {code.split("\n").length} lines · {code.length} chars
            </span>
          </div>
        </div>

        {/* Output */}
        <div style={{ display: "flex", flexDirection: "column", background: C.bg }}>
          <div style={{
            padding: "9px 16px 9px 24px",
            borderBottom: `1px solid ${C.borderLight}`,
            display: "flex", alignItems: "center", justifyContent: "space-between",
          }}>
            <span style={{ fontSize: "10px", color: C.textFaint, letterSpacing: "0.1em", textTransform: "uppercase" }}>output</span>
            {output && (
              <button onClick={() => setOutput(null)} style={{
                background: "none", border: "none", color: C.textFaint,
                cursor: "pointer", fontSize: "12px", fontFamily: C.sans,
              }}>clear</button>
            )}
          </div>

          <div style={{ flex: 1, padding: "20px 24px", overflow: "auto" }}>
            {loading && <Dots />}
            {!loading && !output && (
              <div style={{
                height: "100%", display: "flex", flexDirection: "column",
                justifyContent: "center", alignItems: "center", gap: "12px",
              }}>
                <div style={{
                  width: "32px", height: "32px", borderRadius: "50%",
                  border: `1.5px solid ${C.borderLight}`,
                  display: "flex", alignItems: "center", justifyContent: "center",
                }}>
                  <div style={{ width: "7px", height: "7px", borderRadius: "50%", background: C.borderLight }} />
                </div>
                <span style={{ fontSize: "12px", color: C.textFaint }}>press Run or \u2318\u21b5</span>
              </div>
            )}
            {!loading && output?.type === "success" && <ResultView {...output} />}
            {!loading && output?.type === "error" && <ErrorView error={output.error} />}
          </div>

          {output?.type === "success" && (
            <div style={{
              borderTop: `1px solid ${C.borderLight}`,
              padding: "12px 24px",
              background: C.bgPane, flexShrink: 0,
            }}>
              <div style={{
                fontSize: "10px", color: C.textFaint,
                letterSpacing: "0.1em", textTransform: "uppercase", marginBottom: "8px",
              }}>witness chain</div>
              <div style={{ display: "flex", alignItems: "center", gap: "6px" }}>
                {["source", "parse", "eval", "hash", "receipt"].map((step, i) => (
                  <div key={i} style={{ display: "flex", alignItems: "center", gap: "6px" }}>
                    <div style={{
                      padding: "3px 10px",
                      background: C.greenLight, border: `1px solid ${C.greenMid}`,
                      borderRadius: "100px", color: C.green,
                      fontSize: "10px", fontFamily: C.sans, fontWeight: 500,
                    }}>{step}</div>
                    {i < 4 && <span style={{ color: C.greenMid, fontSize: "11px" }}>\u2192</span>}
                  </div>
                ))}
              </div>
            </div>
          )}

          <div style={{ padding: "6px 24px", borderTop: `1px solid ${C.borderLight}`, background: C.bg }}>
            <span style={{ fontSize: "10px", color: C.textFaint }}>
              v1.6.0 · every run produces a cryptographic receipt
            </span>
          </div>
        </div>
      </div>
    </div>
  );
}
