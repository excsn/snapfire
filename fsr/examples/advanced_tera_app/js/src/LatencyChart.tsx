export default function LatencyChart({ series }: { series: Float64Array }) {
  const points = Array.from(series);
  const avg = points.reduce((a, b) => a + b, 0) / (points.length || 1);
  return (
    <p className="sf-latency">
      latency avg {avg.toFixed(2)}ms over {points.length} samples
    </p>
  );
}
