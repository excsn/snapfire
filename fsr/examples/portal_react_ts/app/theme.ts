import type { Scheme, Theme, ThemeChoice, ThemeCtx } from "@snapfire/fsr/host";

/** A site that forces its theme keeps it. Otherwise `?theme=` previews one for a request, then the visitor's saved pick, then the route's own. The scheme is always the visitor's. */
export function theme({ theme, query, session }: ThemeCtx): ThemeChoice {
  const scheme = session.scheme === "light" || session.scheme === "dark" ? (session.scheme as Scheme) : null;
  if (theme.forced) return { name: theme.name, scheme };
  const picked = query.theme ?? session.theme;
  return { name: picked === "" ? theme.name : (picked as Theme), scheme };
}
