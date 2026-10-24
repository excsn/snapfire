export const size = { width: 1200, height: 630 };
export const alt = "Four evenings, a wall of four phone photos";

export default function Image({ photos }: { photos: unknown[] }) {
  return (
    <div style={{ display: "flex", flexDirection: "column", justifyContent: "center", width: "100%", height: "100%", padding: 96, backgroundColor: "#1c1917", color: "#fafaf9" }}>
      <h1 style={{ fontFamily: "Fraunces", fontSize: 120, margin: 0 }}>Four evenings</h1>
      <p style={{ fontFamily: "Fraunces", fontSize: 40, margin: 0 }}>{photos.length} photos on one wall</p>
    </div>
  );
}
