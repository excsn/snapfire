import { AuthFailure, signIn } from "@snapfire/fsr/client";
import { ctx, expect, load, test } from "@snapfire/fsr/client/testing";

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
  expect(landed.path, "the middleware redirects to the flow, which redirects to the application's page").toEqual("/login?return_to=%2Faccount");
  expect(document.querySelector("main form")?.getAttribute("action"), "and the login page rendered its form").toEqual("/auth/callback");
});

test("a spec signs in through the callback, and every render after it is that user's", async () => {
  const c = portal();
  await load("/account", { ctx: c });
  await fetch("/auth/login?return_to=/account");

  const signedIn = await fetch("/auth/callback", credentials("alice", "wonder"));
  expect(signedIn.status).toEqual(303);
  expect(signedIn.headers.get("location"), "and lands where the flow began").toEqual("/account");

  const account = await load("/account", { ctx: c });
  expect(account.path, "the guard lets the signed-in visitor through").toEqual("/account");
  expect(document.querySelector("[data-field=subject]")?.textContent).toEqual("alice");
  expect(document.querySelector("[data-field=role]")?.textContent, "with the claims the provider carried").toEqual("admin");
});

test("a wrong password is refused and leaves the session anonymous", async () => {
  const c = portal();
  await load("/account", { ctx: c });
  await fetch("/auth/login?return_to=/account");

  const denied = await fetch("/auth/callback", credentials("alice", "wrong"));
  expect(denied.status).toEqual(303);
  expect(denied.headers.get("location")?.startsWith("/login?error=denied"), `back to the login page saying so; ${denied.headers.get("location")}`).toBeTruthy();

  const guarded = await load("/account", { ctx: c });
  expect(guarded.path, "and the guard still sends the visitor to sign in").toEqual("/login?return_to=%2Faccount");
});

test("a callback with no login in progress is refused rather than signing anyone in", async () => {
  const c = portal();
  await load("/login", { ctx: c });
  const stray = await fetch("/auth/callback", credentials("alice", "wonder"));
  expect(stray.status, "a spec starts its journey at /auth/login, the way a link does").toEqual(400);
});

test("signIn runs the flow from the page: the password step is answered and the visitor lands signed in", async () => {
  const c = portal();
  await load("/login", { ctx: c });
  const asked: string[] = [];
  const step = await signIn({ returnTo: "/account", navigate: false, present: ({ kind, data }) => {
    asked.push(`${kind} ${JSON.stringify(data.fields)}`);
    return { user: "alice", password: "wonder" };
  } });
  expect(step).toEqual({ next: "complete", to: "/account" });
  expect(asked, "the provider's one step, a form of two fields").toEqual(['form ["user","password"]']);
  await load("/account", { ctx: c });
  expect(document.querySelector("[data-field=subject]")?.textContent).toEqual("alice");
});

test("signIn throws the refusal; without an answer it hands the step back", async () => {
  const c = portal();
  await load("/login", { ctx: c });
  let refused: unknown = null;
  try {
    await signIn({ returnTo: "/account", navigate: false, present: () => ({ user: "alice", password: "wrong" }) });
  } catch (error) {
    refused = error;
  }
  expect(refused instanceof AuthFailure && refused.kind === "denied", `${refused}`).toBeTruthy();
  const held = await signIn({ returnTo: "/account", navigate: false });
  expect(held.next).toEqual("present");
});
