import { action, fail } from "@snapfire/fsr";
import type { ActionCtx } from "@snapfire/fsr";
import type { SaveTalk } from "@schemas/program";

export const save = action(async ({ input, session, services }: ActionCtx<SaveTalk>) => {
  const talks = await services.program.listTalks();
  const known = talks.some((t) => t.id === input.talk_id);
  if (!known) fail("not_found", "there is no talk with that id");
  session.saved = { ...session.saved, [input.talk_id]: true };
  return { saved: Object.keys(session.saved).length };
});

export const drop = action(async ({ input, session }: ActionCtx<SaveTalk>) => {
  if (!session.saved[input.talk_id]) fail("not_found", "that talk is not on your schedule");
  const kept = Object.keys(session.saved)
    .filter((id) => id !== input.talk_id)
    .reduce((held: Record<string, boolean>, id) => ({ ...held, [id]: true }), {});
  session.saved = kept;
  return { saved: Object.keys(kept).length };
});
