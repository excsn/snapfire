import { assert, ctx, load, screen, test } from "@snapfire/fsr-client/testing";

import LoginPage from "@routes/login/page";
import { assert as check, render } from "@snapfire/fsr-client/testing";

function services() {
  return { fleet: { listAgents: () => [], listAlerts: () => [] } };
}

test("the login page posts the dev credentials to the callback", async () => {
  const c = ctx({ session: { watching: {}, density: "comfortable" }, services: services() });
  await load("/login", { ctx: c });
  const form = document.querySelector("form.signin")!;
  assert.equal(form.getAttribute("method"), "post");
  assert.equal(form.getAttribute("action"), "/auth/callback");
  assert.ok(form.querySelector('input[name="user"]') && form.querySelector('input[name="password"]'));
  assert.equal(document.querySelector(".denied"), null);
  assert.ok(screen.getByText("alice"));
});

test("a denied callback lands back on the page with the message", async () => {
  const c = ctx({ session: { watching: {}, density: "comfortable" }, query: { error: "denied" }, services: services() });
  await load("/login?error=denied", { ctx: c });
  assert.equal(document.querySelector(".denied")?.textContent, "Unknown user or wrong password.");

  const r = await render(<LoginPage denied={false} />, { ctx: ctx() });
  check.equal(r.container.querySelector(".denied"), null);
  r.unmount();
});
