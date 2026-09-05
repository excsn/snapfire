import type { Ctx } from "@snapfire/fsr";

export async function load({ params, services }: Ctx<"/invoice/{id}">) {
  const invoice = await services.ledger.getInvoice({ id: BigInt(params.id) });
  return { invoice };
}

export const meta = ({ data }: { data: { invoice: { customer: string } } }) => ({ title: `${data.invoice.customer} · Billing` });
