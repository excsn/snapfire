import type { MiddlewareCtx } from "@snapfire/fsr/host";

/** Runs after the portal's middleware on every path under /billing, with the identity the portal established. The paths are literal: a site's links and routes carry its prefix. */
export async function middleware({ request, identity, next }: MiddlewareCtx) {
  if (request.path === "/billing/overdue" && !identity?.subject) return Response.redirect("/auth/login?return_to=/billing/overdue", 307);
  const response = await next();
  response.headers.set("x-billing", "invoices");
  return response;
}
