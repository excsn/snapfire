import { assert, ctx, load, test } from "@snapfire/fsr-client/testing";

const teams = [{ name: "Platform", site: "billing", lead: "alice" }];

function portal() {
  return ctx({ services: { directory: { listTeams: () => teams } } });
}

/** The form the login page posts, as the browser encodes it. */
function credentials(user: string, password: string): RequestInit {
  return { method: "POST", headers: { "content-type": "application/x-www-form-urlencoded" }, body: `user=${user}&password=${password}` };
}

test("the guard sends an anonymous visitor through the login route to the login page", async () => {
  const c = portal();
  const landed = await load("/account", { ctx: c });
  assert.equal(landed.path, "/login?return_to=%2Faccount", "the middleware redirects to the flow, which redirects to the application's page");
  assert.equal(document.querySelector(".login form")?.getAttribute("action"), "/auth/callback", "and the login page rendered its form");
});

test("a spec signs in through the callback, and every render after it is that user's", async () => {
  const c = portal();
  await load("/account", { ctx: c });
  await fetch("/auth/login?return_to=/account");

  const signedIn = await fetch("/auth/callback", credentials("alice", "wonder"));
  assert.equal(signedIn.status, 303);
  assert.equal(signedIn.headers.get("location"), "/account", "and lands where the flow began");

  const account = await load("/account", { ctx: c });
  assert.equal(account.path, "/account", "the guard lets the signed-in visitor through");
  assert.equal(document.querySelector(".subject")?.textContent, "alice");
  assert.equal(document.querySelector(".role")?.textContent, "admin", "with the claims the provider carried");
});

test("a wrong password is refused and leaves the session anonymous", async () => {
  const c = portal();
  await load("/account", { ctx: c });
  await fetch("/auth/login?return_to=/account");

  const denied = await fetch("/auth/callback", credentials("alice", "wrong"));
  assert.equal(denied.status, 303);
  assert.ok(denied.headers.get("location")?.startsWith("/login?error=denied"), `back to the login page saying so; ${denied.headers.get("location")}`);

  const guarded = await load("/account", { ctx: c });
  assert.equal(guarded.path, "/login?return_to=%2Faccount", "and the guard still sends the visitor to sign in");
});

test("a callback with no login in progress is refused rather than signing anyone in", async () => {
  const c = portal();
  await load("/login", { ctx: c });
  const stray = await fetch("/auth/callback", credentials("alice", "wonder"));
  assert.equal(stray.status, 400, "a spec starts its journey at /auth/login, the way a link does");
});
