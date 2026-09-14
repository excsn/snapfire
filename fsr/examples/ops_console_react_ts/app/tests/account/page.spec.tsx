import { ctx, expect, load, screen, test } from "@snapfire/fsr-client/testing";

const agents = [{ id: 1n, name: "builder-eu-1", region: "eu", status: "up", queue_depth: 3n, cpu: 61.5 }];

function services() {
  return { fleet: { listAgents: () => agents, listAlerts: () => [] } };
}

test("a signed-in ctx renders the account page and the header shows who it is", async () => {
  const c = ctx({ session: { watching: {}, density: "comfortable" }, identity: { subject: "alice", claims: { role: "admin" } }, services: services() });
  const { status } = await load("/account", { ctx: c });
  expect(status).toEqual(200);
  expect(document.querySelector(".subject")?.textContent).toEqual("alice");
  expect(document.querySelector(".role")?.textContent).toEqual("admin");
  expect(screen.getByText("Sign out"), "the header renders the sign-out form for an identified session").toBeTruthy();
  expect(document.querySelector('form[action="/auth/logout"] input[name="_csrf"]')?.getAttribute("name")).toEqual("_csrf");
  expect(document.querySelector('sf-i[data-sf-module="routes/account/page.tsx#default"]'), "the account page has no state or handlers, so it is markup rather than an island").toBeNull();
});

test("an anonymous ctx sees the sign-in link and no account", async () => {
  const c = ctx({ session: { watching: {}, density: "comfortable" }, services: services() });
  await load("/help", { ctx: c });
  const link = screen.getByText("Sign in");
  expect(link.getAttribute("href")).toEqual("/auth/login");
  expect(link.hasAttribute("data-sf-native"), "a full navigation, since the host answers with a redirect").toBeTruthy();
  expect(document.querySelector('form[action="/auth/logout"]')).toBeNull();
});
