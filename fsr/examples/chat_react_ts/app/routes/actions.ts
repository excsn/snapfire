import { action } from "@snapfire/fsr";
import type { ActionCtx } from "@snapfire/fsr";
import type { NameInput } from "@schemas/inputs";

export const name = action(async ({ input, session }: ActionCtx<NameInput>) => {
  session.name = input.name;
  return { name: input.name };
});
