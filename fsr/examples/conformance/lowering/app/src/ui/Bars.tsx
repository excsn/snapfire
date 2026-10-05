export default function Bars({ rows }: { rows: { name: string; ms: number }[] }) {
  const top = Math.max(...rows.map((r) => r.ms));
  return (
    <ul className="bars">
      {rows.map(({ name, ms }) => (
        <li key={name}>
          <span className="bar-name">{name}</span>
          <span className="bar-track">
            <span className="bar-fill" style={{ width: `${(ms / top) * 100}%`, opacity: ms === top ? 1 : 0.6 }} />
          </span>
          <span className="bar-value" style={{ marginLeft: 8 }}>{ms} ms</span>
        </li>
      ))}
    </ul>
  );
}
