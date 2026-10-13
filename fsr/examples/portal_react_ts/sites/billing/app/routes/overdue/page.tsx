import type { OverdueProps } from "@generated/client";

export default function OverduePage({ overdue, who }: OverdueProps) {
  return (
    <div>
      <h1>Overdue</h1>
      <p className="mb-4 text-ink-soft">Shown to {who}, whose sign-in the portal holds; the site never saw a password.</p>
      <ul className="divide-y divide-line">
        {overdue.map((invoice) => (
          <li key={String(invoice.id)} className="py-2">
            {invoice.customer} owes {invoice.total}
          </li>
        ))}
      </ul>
    </div>
  );
}
