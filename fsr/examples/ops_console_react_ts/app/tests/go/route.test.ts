import { GET } from "@routes/go/[id]/route";
import { ctx, expect, fn, test } from "@snapfire/fsr/testing";

test("a short link redirects and counts the visit once it has answered", async () => {
  const recordVisit = fn(() => ({ count: 1n }));
  const c = ctx<void, "/go/{id}">({ params: { id: "3" }, services: { fleet: { recordVisit } } });
  const response = await GET(c);
  expect(response.status).toEqual(308);
  expect(response.headers.get("location")).toEqual("/agents/3");
  expect(recordVisit).toHaveBeenCalledTimes(1);
  expect(recordVisit).toHaveBeenCalledWith({ id: 3n });
});
