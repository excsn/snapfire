import type { IdProps } from "@generated/client";

export default function ReportPage({ report }: IdProps) {
  return (
    <div className="page report">
      <h1>{report.name}</h1>
      <p className="period">{report.period}</p>
      <table className="report-table">
        <tbody>
          {report.rows.map((row) => (
            <tr key={row.label}>
              <td>{row.label}</td>
              <td className="total">{row.value.toFixed(2)}</td>
            </tr>
          ))}
          <tr className="sum">
            <td>Total</td>
            <td className="total">{report.total.toFixed(2)}</td>
          </tr>
        </tbody>
      </table>
    </div>
  );
}
