import { action } from "@snapfire/fsr";
import type { ActionCtx } from "@snapfire/fsr";
import type { Touch } from "@schemas/ctx";

export const touch = action(async ({ input, session, identity, locale, services }: ActionCtx<Touch>) => {
  const stamped = await services.probe.getStamp();
  session.note = input.note;
  return { note: session.note, subject: identity?.subject ?? "", locale, stamp: stamped.stamp };
});
