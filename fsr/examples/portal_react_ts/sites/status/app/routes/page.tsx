import type { RootProps } from "@generated/client";

const tone: Record<string, string> = {
  operational: "bg-emerald-100 text-emerald-800 dark:bg-emerald-950 dark:text-emerald-300",
  degraded: "bg-amber-100 text-amber-800 dark:bg-amber-950 dark:text-amber-300",
  down: "bg-rose-100 text-rose-800 dark:bg-rose-950 dark:text-rose-300",
};

const cell = "border-b border-line px-2.5 py-2 text-left";

export default function Index({ systems, summary }: RootProps) {
  return (
    <div>
      <h1>Status</h1>
      <p className="mb-4 text-ink-soft">{summary}</p>
      <table className="w-full border-collapse text-sm">
        <thead className="text-xs tracking-wide text-ink-soft uppercase">
          <tr>
            <th className={cell}>System</th>
            <th className={cell}>Owner</th>
            <th className={cell}>State</th>
            <th className={cell}>Note</th>
          </tr>
        </thead>
        <tbody>
          {systems.map((system) => (
            <tr key={system.name} className="hover:bg-accent-soft">
              <td className={`${cell} font-medium`}>{system.name}</td>
              <td className={cell}>{system.owner}</td>
              <td className={cell}>
                <span className={`rounded-full px-2 py-0.5 text-[0.85em] ${tone[system.state]}`}>{system.state}</span>
              </td>
              <td className={`${cell} text-ink-soft`}>{system.note}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
