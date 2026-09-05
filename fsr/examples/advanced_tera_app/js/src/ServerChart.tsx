import { useState } from "react";

const W = 320;
const H = 140;
const PAD = { top: 12, right: 8, bottom: 22, left: 8 };

export default function ServerChart({ series }: { series: Float64Array }) {
  const [selected, setSelected] = useState(-1);
  const points = Array.from(series);
  const max = Math.max(...points, 1);
  const innerW = W - PAD.left - PAD.right;
  const innerH = H - PAD.top - PAD.bottom;
  const slot = innerW / Math.max(points.length, 1);
  const barW = Math.min(slot * 0.6, 48);
  const total = points.reduce((a, b) => a + b, 0);
  return (
    <figure className="sf-chart">
      <svg viewBox={`0 0 ${W} ${H}`} role="img" aria-label={`${points.length} load samples`}>
        <line className="axis" x1={PAD.left} x2={W - PAD.right} y1={H - PAD.bottom} y2={H - PAD.bottom} />
        {points.map((p, i) => {
          const h = (p / max) * innerH;
          const x = PAD.left + slot * i + (slot - barW) / 2;
          const y = H - PAD.bottom - h;
          return (
            <g key={i} onClick={() => setSelected(i === selected ? -1 : i)}>
              <rect className={i === selected ? "bar selected" : "bar"} x={x} y={y} width={barW} height={h} rx={3} />
              <text className="label" x={x + barW / 2} y={y - 4} textAnchor="middle">
                {p}
              </text>
              <text className="label" x={x + barW / 2} y={H - PAD.bottom + 14} textAnchor="middle">
                #{i + 1}
              </text>
            </g>
          );
        })}
      </svg>
      <figcaption>
        <span>{selected >= 0 ? <>sample #{selected + 1}: <b>{points[selected]}</b></> : <>{points.length} samples, total <b>{total}</b></>}</span>
        <span className="badge">island hydrated on load</span>
      </figcaption>
    </figure>
  );
}
