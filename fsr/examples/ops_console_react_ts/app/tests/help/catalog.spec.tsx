import { catalog, setLocale } from "@snapfire/fsr-client";
import { t } from "@snapfire/fsr-client/std";
import { ctx, expect, load, screen, test } from "@snapfire/fsr-client/testing";

function services() {
  return { fleet: { listAgents: () => [], listAlerts: () => [] } };
}

test("t reads the catalog of the document's locale, with plural forms and placeholders", async () => {
  const c = ctx({ session: { watching: {}, density: "comfortable" }, services: services() });
  await load("/fr_FR/help", { ctx: c });
  expect(screen.getByText("Comment ça marche")).toBeTruthy();
  expect(catalog("fr_FR"), "the document embedded the French table").toBeTruthy();
  expect(catalog("fr_FR")?.["help.title"]).toEqual("Comment ça marche");
  expect(t("help.title")).toEqual("Comment ça marche");
  expect(t("agents.watching", { count: 1 })).toEqual("1 agent suivi");
  expect(t("agents.watching", { count: 3 })).toEqual("3 agents suivis");
  expect(t("nothing.here")).toEqual("nothing.here");

  await load("/help", { ctx: c });
  expect(screen.getByText("How this works")).toBeTruthy();
  expect(t("agents.watching", { count: 1 })).toEqual("watching 1 agent");
  setLocale("fr_FR");
  expect(t("help.title"), "the French table is still held after switching back").toEqual("Comment ça marche");
});
