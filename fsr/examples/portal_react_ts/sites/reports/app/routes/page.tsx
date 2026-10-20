import type { RootProps } from "@generated/client";
import { Link } from "@snapfire/fsr/components";

const cell = "border-b border-slate-700 px-1.5 py-2";

export default function ReportsPage({ reports }: RootProps) {
  return (
    <div>
      <h1>Reports</h1>
      <p className="mb-4 text-slate-400">Read from a metrics service this site declares and the portal does not.</p>
      <table className="w-full border-collapse text-left">
        <thead>
          <tr>
            <th className={cell}>Report</th>
            <th className={cell}>Period</th>
            <th className={`${cell} figures`}>Total</th>
          </tr>
        </thead>
        <tbody>
          {reports.map((report) => (
            <tr key={String(report.id)}>
              <td className={cell}>
                <Link href={`/reports/${report.id}`} className="text-sky-400 hover:underline">
                  {report.name}
                </Link>
              </td>
              <td className={cell}>{report.period}</td>
              <td className={`${cell} figures`}>{report.total.toFixed(2)}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
