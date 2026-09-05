import { action, fail } from "@snapfire/fsr";
import type { ActionCtx } from "@snapfire/fsr";
import type { WatchAgent } from "@schemas/fleet";

export const watchAgent = action(async ({ input, session }: ActionCtx<WatchAgent>) => {
  const key = String(input.agent_id);
  if (session.watching[key]) fail("conflict", "already watching that agent");
  session.watching = { ...session.watching, [key]: true };
  return { watching: BigInt(Object.keys(session.watching).length) };
});
