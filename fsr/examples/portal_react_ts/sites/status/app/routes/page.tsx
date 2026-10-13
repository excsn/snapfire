import type { RootProps } from "@generated/client";

const tone: Record<string, string> = {
  operational: "bg-emerald-100 text-emerald-800",
  degraded: "bg-amber-100 text-amber-800",
  down: "bg-rose-100 text-rose-800",
};

const cell = "border-b border-slate-200 px-2.5 py-2 text-left";

export default function Index({ systems, summary }: RootProps) {
  return (
    <div>
      <h1>Status</h1>
      <p className="mb-4 text-slate-600">{summary}</p>
      <table className="w-full border-collapse text-sm">
        <thead className="text-xs tracking-wide text-slate-500 uppercase">
          <tr>
            <th className={cell}>System</th>
            <th className={cell}>Owner</th>
            <th className={cell}>State</th>
            <th className={cell}>Note</th>
          </tr>
        </thead>
        <tbody>
          {systems.map((system) => (
            <tr key={system.name} className="hover:bg-slate-50">
              <td className={`${cell} font-medium`}>{system.name}</td>
              <td className={cell}>{system.owner}</td>
              <td className={cell}>
                <span className={`rounded-full px-2 py-0.5 text-[0.85em] ${tone[system.state]}`}>{system.state}</span>
              </td>
              <td className={`${cell} text-slate-600`}>{system.note}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
