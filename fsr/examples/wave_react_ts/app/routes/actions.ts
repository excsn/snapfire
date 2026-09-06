import { action } from "@snapfire/fsr";
import type { ActionCtx } from "@snapfire/fsr";
import type { BlipInput, NameInput } from "@schemas/inputs";

export const name = action(async ({ input, session }: ActionCtx<NameInput>) => {
  session.name = input.name;
  return { name: input.name };
});

export const blip = action(async ({ input, services, session }: ActionCtx<BlipInput>) => {
  const kept = await services.waves.addBlip({ id: input.wave, parent: input.parent, who: session.name, body: input.body });
  return { kept };
});
