import { action, fail } from "@snapfire/fsr";
import type { ActionCtx } from "@snapfire/fsr";
import type { LookForm } from "@schemas/session";

export const setLook = action(async ({ input, session }: ActionCtx<LookForm>) => {
  if (!["", "ocean", "forest"].includes(input.theme)) fail("invalid", "not a theme");
  if (!["", "light", "dark"].includes(input.scheme)) fail("invalid", "not a scheme");
  session.theme = input.theme;
  session.scheme = input.scheme;
  return { theme: input.theme, scheme: input.scheme };
});
