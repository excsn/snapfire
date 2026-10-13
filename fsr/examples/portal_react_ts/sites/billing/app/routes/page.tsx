import type { RootProps } from "@generated/client";
import { Link, Picture } from "@snapfire/fsr-client/react";

import ledger from "../src/img/ledger.png";

const cell = "border-b border-line px-1.5 py-2";

export default function InvoicesPage({ invoices }: RootProps) {
  return (
    <div>
      <h1>Invoices</h1>
      <Picture src={ledger} alt="" sizes="(max-width: 640px) 100vw, 400px" className="mb-4 w-full max-w-[400px] rounded-lg" />
      <table className="w-full border-collapse text-left">
        <thead>
          <tr>
            <th className={cell}>Customer</th>
            <th className={cell}>Status</th>
            <th className={`${cell} text-right`}>Total</th>
          </tr>
        </thead>
        <tbody>
          {invoices.map((invoice) => (
            <tr key={String(invoice.id)} data-status={invoice.status} className="group">
              <td className={cell}>
                <Link href={`/billing/invoice/${invoice.id}`}>{invoice.customer}</Link>
              </td>
              <td className={`${cell} group-data-[status=overdue]:text-rose-600 group-data-[status=paid]:text-emerald-600`}>{invoice.status}</td>
              <td className={`${cell} text-right font-mono`}>{invoice.total.toFixed(2)}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
