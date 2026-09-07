import { action, fail } from "@snapfire/fsr";
import type { ActionCtx } from "@snapfire/fsr";
import type { AmendInput, BlipInput, NameInput } from "@schemas/inputs";

export const name = action(async ({ input, session }: ActionCtx<NameInput>) => {
  session.name = input.name;
  return { name: input.name };
});

export const blip = action(async ({ input, services, session }: ActionCtx<BlipInput>) => {
  if (!session.name) fail("invalid", "name yourself before writing on a wave");
  const kept = await services.waves.addBlip({ id: input.wave, parent: input.parent, who: session.name, body: input.body });
  return { kept };
});

export const amend = action(async ({ input, services, session }: ActionCtx<AmendInput>) => {
  if (!session.name) fail("invalid", "name yourself before rewriting a blip");
  const amended = await services.waves.editBlip({ id: input.wave, blip: input.blip, who: session.name, body: input.body });
  return { amended };
});
