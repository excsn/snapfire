import type { IndexProps } from "@generated/client";
import { Link } from "@snapfire/fsr-client/react";

export default function InvoicesPage({ invoices }: IndexProps) {
  return (
    <div className="page invoices">
      <h1>Invoices</h1>
      <table className="invoice-table">
        <thead>
          <tr>
            <th>Customer</th>
            <th>Status</th>
            <th>Total</th>
          </tr>
        </thead>
        <tbody>
          {invoices.map((invoice) => (
            <tr key={String(invoice.id)} className={`status-${invoice.status}`}>
              <td>
                <Link href={`/billing/invoice/${invoice.id}`}>{invoice.customer}</Link>
              </td>
              <td>{invoice.status}</td>
              <td className="total">{invoice.total}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
