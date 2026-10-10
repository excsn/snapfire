import { action, fail } from "@snapfire/fsr";
import { t } from "@snapfire/fsr-client/std";
import type { ActionCtx } from "@snapfire/fsr";
import type { SetDensity, WatchAgent } from "@schemas/fleet";

export const unwatchAgent = action(async ({ input, session }: ActionCtx<WatchAgent>) => {
  const key = String(input.agent_id);
  if (!session.watching[key]) fail("not_found", t("agents.not_watching"));
  const kept = Object.keys(session.watching)
    .filter((id) => id !== key)
    .reduce((acc: Record<string, boolean>, id) => ({ ...acc, [id]: true }), {});
  session.watching = kept;
  return { watching: BigInt(Object.keys(kept).length) };
});

export const setDensity = action(async ({ input, session }: ActionCtx<SetDensity>) => {
  if (input.density !== "comfortable" && input.density !== "compact") fail("invalid", t("settings.density_invalid"));
  session.density = input.density;
  return { density: session.density };
});
