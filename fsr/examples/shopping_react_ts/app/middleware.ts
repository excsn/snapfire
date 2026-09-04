import type { MiddlewareCtx, MiddlewareResult } from "@snapfire/fsr";

export async function middleware({ request }: MiddlewareCtx): Promise<MiddlewareResult> {
  if (request.path === "/basket") return { redirect: "/cart" };
  if (request.path === "/shop") return { rewrite: "/" };
  return { headers: { "x-storefront": "fsr" } };
}
