import { useState } from "react";
import { REFUSED, SAMPLES } from "@src/docs/samples";

export function IrInspector() {
  const [activeTab, setActiveTab] = useState<string>(SAMPLES[0].id);
  const [showRefused, setShowRefused] = useState<boolean>(false);

  const current = SAMPLES.find((s) => s.id === activeTab) ?? SAMPLES[0];

  return (
    <div className="inspector-card">
      <div className="inspector-toolbar">
        <div className="sample-tabs">
          {SAMPLES.map((s) => (
            <button
              key={s.id}
              className={activeTab === s.id && !showRefused ? "tab active" : "tab"}
              onClick={() => {
                setActiveTab(s.id);
                setShowRefused(false);
              }}
            >
              {s.name}
            </button>
          ))}
          <button
            className={showRefused ? "tab tab-danger active" : "tab tab-danger"}
            onClick={() => setShowRefused(true)}
          >
            ⚠️ A body it refuses
          </button>
        </div>
      </div>

      <div className="inspector-grid">
        <div className="pane">
          <div className="pane-header">
            <span>Authoring TypeScript</span>
            <span className="badge-lang">TypeScript</span>
          </div>
          <pre className="code-editor">
            <code>{showRefused ? REFUSED.ts : current.ts}</code>
          </pre>
        </div>

        <div className="pane">
          <div className="pane-header">
            <span>{showRefused ? "What the build says" : "Lowered IR"}</span>
            <span className={showRefused ? "badge-lang badge-error" : "badge-lang"}>
              {showRefused ? "Refused" : "plan.json"}
            </span>
          </div>
          <pre className={`code-editor ${showRefused ? "error-output" : ""}`}>
            <code>{showRefused ? REFUSED.diagnostic : current.ir}</code>
          </pre>
        </div>
      </div>

      <div className="inspector-footer">
        <p className="explanation">
          {showRefused
            ? "FSR never silently falls back to a slow engine. A body it cannot lower stops the build, on the line, with the rewrite named."
            : current.explanation}
        </p>
      </div>
    </div>
  );
}
