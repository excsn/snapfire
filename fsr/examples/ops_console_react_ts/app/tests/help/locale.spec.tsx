import { currentLocale } from "@snapfire/fsr-client";
import { assert, ctx, load, render, screen, test } from "@snapfire/fsr-client/testing";

import Help from "@routes/help/page";

function services() {
  return { fleet: { listAgents: () => [], listAlerts: () => [] } };
}

test("a prefixed document loads in French, marks the html element and hydrates the page reading the locale", async () => {
  const c = ctx({ session: { watching: {}, density: "comfortable" }, services: services() });
  await load("/fr_FR/help", { ctx: c });
  assert.equal(document.documentElement.getAttribute("lang"), "fr-FR");
  assert.equal(document.documentElement.getAttribute("data-sf-locale"), "fr_FR");
  assert.equal(currentLocale(), "fr_FR");
  assert.ok(screen.getByText("Comment ça marche"));
  assert.ok(document.querySelector('sf-i[data-sf-module="routes/help/page.tsx#default"][data-sf-mounted]'), "the page hydrated against the locale the server wrote");

  await load("/help", { ctx: c });
  assert.equal(document.documentElement.getAttribute("lang"), "en-US");
  assert.equal(currentLocale(), "en_US");
  assert.ok(screen.getByText("How this works"));
});

test("a component renders under the locale its ctx names, and the host's default without one", async () => {
  const french = await render(<Help />, { ctx: ctx({ locale: "fr_FR" }) });
  assert.equal(french.hydrated, "routes/help/page.tsx#default", "the server rendered it in French and React hydrated over that");
  assert.equal(french.container.querySelector("h1")?.textContent, "Comment ça marche");
  assert.equal(ctx({ locale: "fr_FR" }).locale, "fr_FR");
  assert.equal(ctx().locale, "en_US");
  french.unmount();

  const english = await render(<Help />, { ctx: ctx() });
  assert.equal(english.hydrated, "routes/help/page.tsx#default");
  assert.equal(english.container.querySelector("h1")?.textContent, "How this works");
  english.unmount();
});
