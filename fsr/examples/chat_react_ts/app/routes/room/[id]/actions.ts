import { action } from "@snapfire/fsr";
import type { ActionCtx } from "@snapfire/fsr";
import type { SayInput } from "@schemas/inputs";

export const say = action(async ({ input, services, session }: ActionCtx<SayInput>) => {
  const kept = await services.rooms.postMessage({ id: input.room, who: session.name, body: input.body });
  return { kept };
});
