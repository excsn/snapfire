import type { Ctx } from "@snapfire/fsr";

export async function load({ services, identity }: Ctx) {
  const invoices = await services.ledger.listInvoices();
  return { overdue: invoices.filter((invoice) => invoice.status === "overdue"), who: identity?.subject ?? "" };
}

export const meta = () => ({ title: "Overdue · Billing" });
