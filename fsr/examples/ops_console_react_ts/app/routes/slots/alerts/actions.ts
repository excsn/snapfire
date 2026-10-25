import { action } from "@snapfire/fsr/host";
import type { ActionCtx } from "@snapfire/fsr/host";
import type { AckAlert } from "@schemas/fleet";

export const ackAlert = action(async ({ input, services }: ActionCtx<AckAlert>) => {
  const left = await services.fleet.acknowledgeAlert({ id: input.alert_id });
  return { open: BigInt(left.length) };
});
