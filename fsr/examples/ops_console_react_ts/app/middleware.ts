import type { MiddlewareCtx, MiddlewareResult } from "@snapfire/fsr";

export async function middleware({ request, identity }: MiddlewareCtx): Promise<MiddlewareResult> {
  if (request.path === "/dashboard") return { redirect: "/" };
  if (request.path === "/fleet") return { rewrite: "/agents" };
  if (request.path === "/account" && !identity?.subject) return { redirect: "/auth/login?return_to=/account" };
  return { headers: { "x-ops-console": "fsr" } };
}
