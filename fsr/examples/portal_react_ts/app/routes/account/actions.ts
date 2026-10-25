import { action, fail } from "@snapfire/fsr/host";
import type { ActionCtx } from "@snapfire/fsr/host";
import type { LookForm, SchemeChoice, ThemeChoice } from "@schemas/session";

export const setScheme = action(async ({ input, session }: ActionCtx<SchemeChoice>) => {
  if (!["", "light", "dark"].includes(input.scheme)) fail("invalid", "not a scheme");
  session.scheme = input.scheme;
  return { scheme: input.scheme };
});

export const setTheme = action(async ({ input, session }: ActionCtx<ThemeChoice>) => {
  if (!["", "default", "ocean", "forest"].includes(input.theme)) fail("invalid", "not a theme");
  session.theme = input.theme;
  return { theme: input.theme };
});

export const setLook = action(async ({ input, session }: ActionCtx<LookForm>) => {
  if (!["", "default", "ocean", "forest"].includes(input.theme)) fail("invalid", "not a theme");
  if (!["", "light", "dark"].includes(input.scheme)) fail("invalid", "not a scheme");
  session.theme = input.theme;
  session.scheme = input.scheme;
  return { theme: input.theme, scheme: input.scheme };
});
