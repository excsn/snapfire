import type { RootProps } from "@generated/client";

export const size = { width: 1200, height: 630 };
export const alt = "Acme portal: the teams and the sites they own";

export default function Image({ teams }: RootProps) {
  return (
    <div style={{ display: "flex", flexDirection: "column", justifyContent: "space-between", width: "100%", height: "100%", padding: 72, backgroundImage: "linear-gradient(135deg, #0b1020 0%, #1d2b64 100%)", color: "white" }}>
      <div style={{ display: "flex", alignItems: "center", gap: 20 }}>
        <div style={{ width: 56, height: 56, borderRadius: 14, backgroundColor: "#f59e0b" }} />
        <span style={{ fontSize: 34, fontWeight: 700 }}>Acme portal</span>
      </div>
      <h1 style={{ fontSize: 76, fontWeight: 700, lineHeight: 1.1, margin: 0 }}>{teams.length} teams, one portal</h1>
      <p style={{ fontSize: 30, opacity: 0.75, margin: 0 }}>{teams.map((team) => team.name).join(" · ")}</p>
    </div>
  );
}
