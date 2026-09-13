import { action, fail } from "@snapfire/fsr";
import type { ActionCtx } from "@snapfire/fsr";
import type { AmendInput, BlipInput, NameInput, PlayInput, ResetInput } from "@schemas/inputs";

export const name = action(async ({ input, session }: ActionCtx<NameInput>) => {
  session.name = input.name;
  return { name: input.name };
});

export const blip = action(async ({ input, services, session }: ActionCtx<BlipInput>) => {
  if (!session.name) fail("invalid", "name yourself before writing on a wave");
  const kept = await services.waves.addBlip({ id: input.wave, parent: input.parent, anchor: input.anchor, who: session.name, body: input.body });
  return { kept };
});

/** A move in the wave's gadget. Everything a move is happens here and in the
 * field beneath it: whose turn, whether the cell is free, whether that ended
 * it. The gadget's handler does nothing but name this. */
export const play = action(async ({ input, services, session }: ActionCtx<PlayInput>) => {
  if (!session.name) fail("invalid", "name yourself before playing");
  const game = await services.waves.play({ id: input.wave, who: session.name, cell: input.cell });
  return { game };
});

export const reset = action(async ({ input, services, session }: ActionCtx<ResetInput>) => {
  if (!session.name) fail("invalid", "name yourself before playing");
  const game = await services.waves.play({ id: input.wave, who: session.name });
  return { game };
});

export const amend = action(async ({ input, services, session }: ActionCtx<AmendInput>) => {
  if (!session.name) fail("invalid", "name yourself before rewriting a blip");
  const amended = await services.waves.editBlip({ id: input.wave, blip: input.blip, who: session.name, body: input.body });
  return { amended };
});
