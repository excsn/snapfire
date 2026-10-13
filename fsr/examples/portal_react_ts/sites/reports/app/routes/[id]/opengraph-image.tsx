import type { IdProps } from "@generated/client";

export const size = { width: 1200, height: 630 };
export const contentType = "image/png";

export default function Image({ report }: IdProps) {
  return (
    <div style={{ display: "flex", flexDirection: "column", width: "100%", height: "100%", padding: 72, gap: 32, backgroundColor: "#0f172a", color: "#e2e8f0" }}>
      <span style={{ fontSize: 30, fontWeight: 700, color: "#38bdf8" }}>Acme reports</span>
      <h1 style={{ fontSize: 68, fontWeight: 700, margin: 0, color: "white" }}>{report.name}</h1>
      <p style={{ fontSize: 30, margin: 0 }}>{report.period}</p>
      <div style={{ display: "flex", flexDirection: "column", gap: 12, marginTop: "auto" }}>
        {report.rows.map((row) => (
          <div key={row.label} style={{ display: "flex", justifyContent: "space-between", fontSize: 30, borderBottom: "1px solid #334155", paddingBottom: 8 }}>
            <span>{row.label}</span>
            <span>{row.value.toFixed(2)}</span>
          </div>
        ))}
        <div style={{ display: "flex", justifyContent: "space-between", fontSize: 34, fontWeight: 700, color: "white" }}>
          <span>Total</span>
          <span>{report.total.toFixed(2)}</span>
        </div>
      </div>
    </div>
  );
}
