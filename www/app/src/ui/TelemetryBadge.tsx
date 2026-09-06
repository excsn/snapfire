import { useState } from "react";

export function TelemetryBadge({ initialHost }: { initialHost: string }) {
  const [active, setActive] = useState(false);

  return (
    <div className="telemetry-box">
      <span className="telemetry-pill">
        Runtime: <strong>{initialHost}</strong>
      </span>
      <button 
        className="btn-telemetry" 
        onClick={() => setActive(!active)}
      >
        {active ? "Hide Runtime Stats" : "Inspect Server Seams"}
      </button>

      {active ? (
        <ul className="telemetry-details">
          <li>Server Engine: Tokio + Actix (Rust)</li>
          <li>SSR Evaluation: Native Rust AST Interpreter</li>
          <li>Server JS Footprint: 0 KB</li>
          <li>Hydration Mismatch Count: 0</li>
        </ul>
      ) : null}
    </div>
  );
}