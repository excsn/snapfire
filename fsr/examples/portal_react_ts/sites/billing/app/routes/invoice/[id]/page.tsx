import type { InvoiceIdProps } from "@generated/client";

import { PayButton } from "@src/ui/PayButton";

export default function InvoicePage({ invoice }: InvoiceIdProps) {
  return (
    <div className="space-y-4">
      <h1>{invoice.customer}</h1>
      <dl className="grid grid-cols-[max-content_1fr] gap-x-4 gap-y-1.5 [&_dt]:text-slate-500">
        <dt>Invoice</dt>
        <dd>#{String(invoice.id)}</dd>
        <dt>Status</dt>
        <dd>{invoice.status}</dd>
        <dt>Total</dt>
        <dd>{invoice.total}</dd>
      </dl>
      <PayButton id={invoice.id} status={invoice.status} />
    </div>
  );
}
