import { ctx, expect, load, screen, test } from "@snapfire/fsr-client/testing";

import LoginPage from "@routes/login/page";
import { render } from "@snapfire/fsr-client/testing";

function services() {
  return { fleet: { listAgents: () => [], listAlerts: () => [] } };
}

test("the login page posts the dev credentials to the callback", async () => {
  const c = ctx({ session: { watching: {}, density: "comfortable" }, services: services() });
  await load("/login", { ctx: c });
  const form = document.querySelector("form.signin")!;
  expect(form.getAttribute("method")).toEqual("post");
  expect(form.getAttribute("action")).toEqual("/auth/callback");
  expect(form.querySelector('input[name="user"]') && form.querySelector('input[name="password"]')).toBeTruthy();
  expect(document.querySelector(".denied")).toBeNull();
  expect(screen.getByText("alice")).toBeTruthy();
});

test("a denied callback lands back on the page with the message", async () => {
  const c = ctx({ session: { watching: {}, density: "comfortable" }, query: { error: "denied" }, services: services() });
  await load("/login?error=denied", { ctx: c });
  expect(document.querySelector(".denied")?.textContent).toEqual("Unknown user or wrong password.");

  const r = await render(<LoginPage denied={false} />, { ctx: ctx() });
  expect(r.container.querySelector(".denied")).toBeNull();
  r.unmount();
});
