import { useState } from "react";

export default function ServerChart({ series }: { series: Float64Array }) {
  const [selected, setSelected] = useState(-1);
  const points = Array.from(series);
  const max = Math.max(...points, 1);
  return (
    <figure className="sf-chart">
      <figcaption>{selected >= 0 ? `point ${selected}: ${points[selected]}` : `${points.length} points`}</figcaption>
      {points.map((p, i) => (
        <span
          key={i}
          onClick={() => setSelected(i)}
          style={{ display: "inline-block", width: "14px", background: "#69c", height: `${(p / max) * 60}px` }}
        />
      ))}
    </figure>
  );
}
