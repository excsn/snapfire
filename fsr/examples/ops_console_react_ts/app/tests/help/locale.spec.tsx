import { currentLocale, localePath } from "@snapfire/fsr-client";
import { ctx, expect, load, render, screen, test } from "@snapfire/fsr-client/testing";

import Help from "@routes/help/page";

function services() {
  return { fleet: { listAgents: () => [], listAlerts: () => [] } };
}

test("a prefixed document loads in French, marks the html element and renders the page in that locale", async () => {
  const c = ctx({ session: { watching: {}, density: "comfortable" }, services: services() });
  await load("/fr_FR/help", { ctx: c });
  expect(document.documentElement.getAttribute("lang")).toEqual("fr-FR");
  expect(document.documentElement.getAttribute("data-sf-locale")).toEqual("fr_FR");
  expect(currentLocale()).toEqual("fr_FR");
  expect(screen.getByText("Comment ça marche")).toBeTruthy();
  expect(document.querySelector('sf-i[data-sf-module="routes/help/page.tsx#default"]'), "the help page has no state or handlers, so its French markup is what the server wrote").toBeNull();

  await load("/help", { ctx: c });
  expect(document.documentElement.getAttribute("lang")).toEqual("en-US");
  expect(currentLocale()).toEqual("en_US");
  expect(screen.getByText("How this works")).toBeTruthy();
});

test("a component renders under the locale its ctx names, and the host's default without one", async () => {
  const french = await render(<Help />, { ctx: ctx({ locale: "fr_FR" }) });
  expect(french.hydrated, "the help page is static, so render mounts it fresh in the locale its ctx names").toBeNull();
  expect(french.container.querySelector("h1")?.textContent).toEqual("Comment ça marche");
  expect(ctx({ locale: "fr_FR" }).locale).toEqual("fr_FR");
  expect(ctx().locale).toEqual("en_US");
  french.unmount();

  const english = await render(<Help />, { ctx: ctx() });
  expect(english.hydrated).toBeNull();
  expect(english.container.querySelector("h1")?.textContent).toEqual("How this works");
  english.unmount();
});

test("switching locale keeps the page the reader is on, not the one the switcher lives on", async () => {
  const c = ctx({ session: { watching: {}, density: "comfortable" }, services: services() });

  await load("/help", { ctx: c });
  expect(localePath("fr_FR"), "an unprefixed document takes the prefix").toEqual("/fr_FR/help");

  await load("/fr_FR/help", { ctx: c });
  expect(localePath("en_US"), "a prefixed one swaps it rather than stacking").toEqual("/en_US/help");
  expect(localePath("fr_FR"), "and choosing the locale it is already in is the same page").toEqual("/fr_FR/help");

  expect(localePath("fr_FR", "/agents?region=eu"), "a path given explicitly keeps its query").toEqual("/fr_FR/agents?region=eu");
  expect(localePath("fr_FR", "/fr_FR"), "the root under a prefix is the prefix").toEqual("/fr_FR");
});
