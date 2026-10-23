import type { MiddlewareCtx } from "@snapfire/fsr";

export async function middleware({ request, next }: MiddlewareCtx) {
  if (request.path === "/basket") return Response.redirect("/cart", 307);
  if (request.path === "/shop") return next(new Request("/"));
  const response = await next();
  response.headers.set("x-storefront", "fsr");
  return response;
}
