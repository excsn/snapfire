import { action } from "@snapfire/fsr/host";
import type { ActionCtx } from "@snapfire/fsr/host";
import type { Pay } from "@schemas/inputs";

export const pay = action(async ({ input, services }: ActionCtx<Pay>) => {
  const invoice = await services.ledger.payInvoice({ id: input.id });
  return { status: invoice.status };
});
