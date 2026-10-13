import type { IdProps } from "@generated/client";

export const size = { width: 1200, height: 600 };

export function images() {
  return [
    { id: "total", alt: "The report's total" },
    { id: "rows", alt: "The report's rows", size: { width: 1200, height: 900 } },
  ];
}

export default function Image({ report, id }: IdProps & { id: string }) {
  return (
    <div style={{ display: "flex", flexDirection: "column", justifyContent: "center", width: "100%", height: "100%", padding: 72, gap: 24, backgroundColor: "#0f172a", color: "white" }}>
      <span style={{ fontSize: 30, color: "#38bdf8" }}>{report.name}</span>
      {id === "total" ? (
        <h1 style={{ fontSize: 120, fontWeight: 700, margin: 0 }}>{report.total.toFixed(2)}</h1>
      ) : (
        <div style={{ display: "flex", flexDirection: "column", gap: 16 }}>
          {report.rows.map((row) => (
            <div key={row.label} style={{ display: "flex", justifyContent: "space-between", fontSize: 44 }}>
              <span>{row.label}</span>
              <span>{row.value.toFixed(2)}</span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
