import type { MiddlewareCtx } from "@snapfire/fsr/host";

/** The portal's rules hold on every route, a mounted site's included: `request.site` names the site a path belongs to or is absent on the portal's own routes. */
export async function middleware({ request, identity, next }: MiddlewareCtx) {
  if (request.path === "/account" && !identity?.subject) return Response.redirect("/auth/login?return_to=/account", 307);
  const response = await next();
  response.headers.set("x-portal", request.site ?? "portal");
  return response;
}
