import type { RootProps } from "@generated/client";
import { Link } from "@snapfire/fsr-client/react";

export default function ReportsPage({ reports }: RootProps) {
  return (
    <div className="page reports-index">
      <h1>Reports</h1>
      <p className="lede">Read from a metrics service only this process can reach. The portal renders this page inside its own document.</p>
      <table className="report-table">
        <thead>
          <tr>
            <th>Report</th>
            <th>Period</th>
            <th>Total</th>
          </tr>
        </thead>
        <tbody>
          {reports.map((report) => (
            <tr key={String(report.id)}>
              <td>
                <Link href={`/reports/${report.id}`}>{report.name}</Link>
              </td>
              <td>{report.period}</td>
              <td className="total">{report.total.toFixed(2)}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
