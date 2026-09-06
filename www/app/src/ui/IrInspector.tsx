import { useState } from "react";

interface Sample {
  id: string;
  name: string;
  tsCode: string;
  irCode: string;
  explanation: string;
}

const SAMPLES: Sample[] = [
  {
    id: "reduce",
    name: "Cart Counter (Functional Map/Reduce)",
    tsCode: `export async function load({ session }: Ctx) {
  const count = Object.values(session.cart)
    .reduce((n, q) => n + q, 0n);
  return { count };
}`,
    irCode: `{
  "let": {
    "name": "count",
    "expr": {
      "reduce": [
        { "values": { "coalesce": [{ "session": "cart" }, { "object": [] }] } },
        { "lit": { "int": 0 } },
        {
          "lambda": {
            "params": ["n", "q"],
            "body": { "arith": ["add", { "var": "n" }, { "var": "q" }] }
          }
        }
      ]
    }
  }
}`,
    explanation: "TypeScript array methods are lowered into deterministic AST nodes. Evaluated natively in Rust with zero V8 heap allocation."
  },
  {
    id: "guard",
    name: "Action Guard (Short-circuiting)",
    tsCode: `export const checkout = action(async ({ session, services }: ActionCtx) => {
  if (Object.keys(session.cart).length === 0) {
    fail("invalid", "the cart is empty");
  }
  return await services.shopping.placeOrder({ lines });
});`,
    irCode: `{
  "guard": {
    "cond": {
      "compare": [
        "eq",
        { "length": { "keys": { "session": "cart" } } },
        { "lit": { "int": 0 } }
      ]
    },
    "kind": "invalid",
    "message": "the cart is empty"
  }
}`,
    explanation: "Guards that do not depend on external I/O run before any service socket is opened, stopping invalid calls before leaving the server."
  },
  {
    id: "server_island",
    name: "Server Island Click Step",
    tsCode: `// Inside an Island placed with mode="server"
<button onClick={() => setOpen(!open)}>
  {open ? "Close" : "Open"}
</button>`,
    irCode: `{
  "handler": {
    "event": "click",
    "body": [
      {
        "return": {
          "object": [
            { "field": ["open", { "not": { "var": "open" } }] }
          ]
        }
      }
    ]
  }
}`,
    explanation: "Server islands compile state transitions into pure data. The browser ships no component JS—clicks trigger a lightweight roundtrip and DOM morph."
  }
];

export function IrInspector() {
  const [activeTab, setActiveTab] = useState<string>("reduce");
  const [simulateResidue, setSimulateResidue] = useState<boolean>(false);

  const current = SAMPLES.find((s) => s.id === activeTab) ?? SAMPLES[0];

  return (
    <div className="inspector-card">
      <div className="inspector-toolbar">
        <div className="sample-tabs">
          {SAMPLES.map((s) => (
            <button
              key={s.id}
              className={activeTab === s.id && !simulateResidue ? "tab active" : "tab"}
              onClick={() => {
                setActiveTab(s.id);
                setSimulateResidue(false);
              }}
            >
              {s.name}
            </button>
          ))}
          <button
            className={simulateResidue ? "tab tab-danger active" : "tab tab-danger"}
            onClick={() => setSimulateResidue(true)}
          >
            ⚠️ Residue Error Demo
          </button>
        </div>
      </div>

      <div className="inspector-grid">
        {/* Left pane: Source TypeScript */}
        <div className="pane">
          <div className="pane-header">
            <span>Authoring TypeScript</span>
            <span className="badge-lang">TypeScript</span>
          </div>
          <pre className="code-editor">
            <code>
              {simulateResidue
                ? `export async function load({ request }: Ctx) {
  // Residue: Imperative while loop or raw fetch
  while (true) {
    doSomething();
  }
  return { status: "ok" };
}`
                : current.tsCode}
            </code>
          </pre>
        </div>

        {/* Right pane: Lowered Plan IR */}
        <div className="pane">
          <div className="pane-header">
            <span>{simulateResidue ? "Compiler Feedback" : "Lowered IR (plan.json)"}</span>
            <span className={simulateResidue ? "badge-lang badge-error" : "badge-lang"}>
              {simulateResidue ? "Residue Rejection" : "Rust AST"}
            </span>
          </div>
          <pre className={`code-editor ${simulateResidue ? "error-output" : ""}`}>
            <code>
              {simulateResidue
                ? `error: residue detected in body
 --> app/routes/demo/page.loader.ts:3:3: \`while\`
  |
3 |   while (true) {
  |   ^^^^^ a construct the IR language cannot represent
  = note: bodies are pure deterministic data; no imperative runtimes in serving path.`
                : current.irCode}
            </code>
          </pre>
        </div>
      </div>

      <div className="inspector-footer">
        <p className="explanation">
          {simulateResidue
            ? "FSR never silently falls back to a slow engine. If a body cannot be lowered, the build stops and reports the line."
            : current.explanation}
        </p>
      </div>
    </div>
  );
}