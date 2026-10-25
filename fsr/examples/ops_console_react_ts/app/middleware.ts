import type { MiddlewareCtx } from "@snapfire/fsr/host";

export async function middleware({ request, identity, next }: MiddlewareCtx) {
  if (request.path === "/dashboard") return Response.redirect("/", 307);
  if (request.path === "/fleet") return next(new Request("/agents"));
  if (request.path === "/account" && !identity?.subject) return Response.redirect("/auth/login?return_to=/account", 307);
  const response = await next();
  response.headers.set("x-ops-console", "fsr");
  return response;
}
