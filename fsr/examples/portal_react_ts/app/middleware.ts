import type { MiddlewareCtx, MiddlewareResult } from "@snapfire/fsr";

/** The portal's rules hold on every route, a mounted site's included: `request.site` names the site a path belongs to, or is absent on the portal's own routes. */
export async function middleware({ request, identity }: MiddlewareCtx): Promise<MiddlewareResult> {
  if (request.path === "/account" && !identity?.subject) return { redirect: "/auth/login?return_to=/account" };
  return { headers: { "x-portal": request.site ?? "portal" } };
}
