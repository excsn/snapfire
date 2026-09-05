import type { MiddlewareCtx, MiddlewareResult } from "@snapfire/fsr";

export async function middleware({ request }: MiddlewareCtx): Promise<MiddlewareResult> {
  if (request.path === "/dashboard") return { redirect: "/" };
  if (request.path === "/fleet") return { rewrite: "/agents" };
  return { headers: { "x-ops-console": "fsr" } };
}
