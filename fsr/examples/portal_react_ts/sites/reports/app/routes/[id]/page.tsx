import type { IdProps } from "@generated/client";

const cell = "border-b border-slate-700 px-1.5 py-2";

export default function ReportPage({ report }: IdProps) {
  return (
    <div>
      <h1>{report.name}</h1>
      <p className="mb-4 text-slate-400">{report.period}</p>
      <table className="w-full border-collapse text-left">
        <tbody>
          {report.rows.map((row) => (
            <tr key={row.label}>
              <td className={cell}>{row.label}</td>
              <td className={`${cell} figures`}>{row.value.toFixed(2)}</td>
            </tr>
          ))}
          <tr className="font-semibold text-white">
            <td className={cell}>Total</td>
            <td className={`${cell} figures`}>{report.total.toFixed(2)}</td>
          </tr>
        </tbody>
      </table>
    </div>
  );
}
