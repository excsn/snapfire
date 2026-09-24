import type { OverdueProps } from "@generated/client";

export default function OverduePage({ overdue, who }: OverdueProps) {
  return (
    <div className="page overdue">
      <h1>Overdue</h1>
      <p className="lede">Shown to {who}, whose sign-in the portal holds; the site never saw a password.</p>
      <ul className="overdue-list">
        {overdue.map((invoice) => (
          <li key={String(invoice.id)}>
            {invoice.customer} owes {invoice.total}
          </li>
        ))}
      </ul>
    </div>
  );
}
