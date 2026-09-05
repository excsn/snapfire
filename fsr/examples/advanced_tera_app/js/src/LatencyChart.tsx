const W = 640;
const H = 140;
const PAD = { top: 24, right: 30, bottom: 22, left: 30 };

export default function LatencyChart({ series }: { series: Float64Array }) {
  const points = Array.from(series);
  const max = Math.max(...points, 1);
  const avg = points.reduce((a, b) => a + b, 0) / (points.length || 1);
  const innerW = W - PAD.left - PAD.right;
  const innerH = H - PAD.top - PAD.bottom;
  const step = points.length > 1 ? innerW / (points.length - 1) : 0;
  const xy = points.map((p, i) => [PAD.left + step * i, H - PAD.bottom - (p / max) * innerH] as const);
  const path = xy.map(([x, y], i) => `${i === 0 ? "M" : "L"}${x.toFixed(1)},${y.toFixed(1)}`).join(" ");
  const area = `${path} L${(PAD.left + step * (points.length - 1)).toFixed(1)},${H - PAD.bottom} L${PAD.left},${H - PAD.bottom} Z`;
  const avgY = H - PAD.bottom - (avg / max) * innerH;
  return (
    <figure className="sf-chart sf-latency">
      <svg viewBox={`0 0 ${W} ${H}`} role="img" aria-label={`${points.length} latency samples`}>
        <line className="axis" x1={PAD.left} x2={W - PAD.right} y1={H - PAD.bottom} y2={H - PAD.bottom} />
        {points.length > 1 && <path className="area" d={area} />}
        <path className="line" d={path} />
        <line className="avg" x1={PAD.left} x2={W - PAD.right} y1={avgY} y2={avgY} />
        {xy.map(([x, y], i) => (
          <g key={i}>
            <circle className="dot" cx={x} cy={y} r={4} />
            <text className="label" x={x} y={y - 9} textAnchor="middle">
              {points[i]}ms
            </text>
            <text className="label" x={x} y={H - PAD.bottom + 14} textAnchor="middle">
              t{i + 1}
            </text>
          </g>
        ))}
      </svg>
      <figcaption>
        <span>
          avg <b>{avg.toFixed(2)}ms</b> over {points.length} samples
        </span>
        <span className="badge">island hydrated when visible</span>
      </figcaption>
    </figure>
  );
}
