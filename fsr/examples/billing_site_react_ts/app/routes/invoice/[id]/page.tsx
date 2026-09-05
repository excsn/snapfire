import type { InvoiceIdProps } from "@generated/client";

import { PayButton } from "@src/ui/PayButton";

export default function InvoicePage({ invoice }: InvoiceIdProps) {
  return (
    <div className="page invoice">
      <h1>{invoice.customer}</h1>
      <dl className="facts">
        <dt>Invoice</dt>
        <dd>#{String(invoice.id)}</dd>
        <dt>Status</dt>
        <dd className="status">{invoice.status}</dd>
        <dt>Total</dt>
        <dd>{invoice.total}</dd>
      </dl>
      <PayButton id={invoice.id} status={invoice.status} />
    </div>
  );
}
