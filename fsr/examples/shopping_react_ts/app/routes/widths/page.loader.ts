import type { Ctx } from "@snapfire/fsr";

export async function load({ services }: Ctx<"/widths">) {
  const ledger = await services.shopping.getLedger();
  const digits = String(ledger.sequence);
  return { ledger, digits, lossy: Number(digits) };
}

export const meta = () => ({ title: "Widths · Shopping" });
