import type { MiddlewareCtx, MiddlewareResult } from "@snapfire/fsr";

/** Runs after the portal's middleware on every path under /billing, with the identity the portal established. The paths are literal: a site's links and routes carry its prefix. */
export async function middleware({ request, identity }: MiddlewareCtx): Promise<MiddlewareResult> {
  if (request.path === "/billing/overdue" && !identity?.subject) return { redirect: "/auth/login?return_to=/billing/overdue" };
  return { headers: { "x-billing": "invoices" } };
}
