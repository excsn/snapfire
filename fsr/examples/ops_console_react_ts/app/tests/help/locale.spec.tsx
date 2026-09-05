import { currentLocale, localePath } from "@snapfire/fsr-client";
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

test("switching locale keeps the page the reader is on, not the one the switcher lives on", async () => {
  const c = ctx({ session: { watching: {}, density: "comfortable" }, services: services() });

  await load("/help", { ctx: c });
  assert.equal(localePath("fr_FR"), "/fr_FR/help", "an unprefixed document takes the prefix");

  await load("/fr_FR/help", { ctx: c });
  assert.equal(localePath("en_US"), "/en_US/help", "a prefixed one swaps it rather than stacking");
  assert.equal(localePath("fr_FR"), "/fr_FR/help", "and choosing the locale it is already in is the same page");

  assert.equal(localePath("fr_FR", "/agents?region=eu"), "/fr_FR/agents?region=eu", "a path given explicitly keeps its query");
  assert.equal(localePath("fr_FR", "/fr_FR"), "/fr_FR", "the root under a prefix is the prefix");
});
