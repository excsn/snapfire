import type { Ctx } from "@snapfire/fsr";

export async function load({ services }: Ctx) {
  const invoices = await services.ledger.listInvoices();
  return { invoices };
}

export const meta = () => ({ title: "Invoices · Billing" });
