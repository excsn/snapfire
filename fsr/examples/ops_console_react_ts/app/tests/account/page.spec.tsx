import { assert, ctx, load, screen, test } from "@snapfire/fsr-client/testing";

const agents = [{ id: 1n, name: "builder-eu-1", region: "eu", status: "up", queue_depth: 3n, cpu: 61.5 }];

function services() {
  return { fleet: { listAgents: () => agents, listAlerts: () => [] } };
}

test("a signed-in ctx renders the account page and the header shows who it is", async () => {
  const c = ctx({ session: { watching: {}, density: "comfortable" }, identity: { subject: "alice", claims: { role: "admin" } }, services: services() });
  const { status } = await load("/account", { ctx: c });
  assert.equal(status, 200);
  assert.equal(document.querySelector(".subject")?.textContent, "alice");
  assert.equal(document.querySelector(".role")?.textContent, "admin");
  assert.ok(screen.getByText("Sign out"), "the header renders the sign-out form for an identified session");
  assert.equal(document.querySelector('form[action="/auth/logout"] input[name="_csrf"]')?.getAttribute("name"), "_csrf");
  assert.ok(document.querySelector('sf-i[data-sf-module="routes/account/page.tsx#default"][data-sf-mounted]'), "the page hydrated");
});

test("an anonymous ctx sees the sign-in link and no account", async () => {
  const c = ctx({ session: { watching: {}, density: "comfortable" }, services: services() });
  await load("/help", { ctx: c });
  const link = screen.getByText("Sign in");
  assert.equal(link.getAttribute("href"), "/auth/login");
  assert.ok(link.hasAttribute("data-sf-native"), "a full navigation, since the host answers with a redirect");
  assert.equal(document.querySelector('form[action="/auth/logout"]'), null);
});
