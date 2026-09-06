import { catalog, setLocale } from "@snapfire/fsr-client";
import { t } from "@snapfire/fsr-client/std";
import { assert, ctx, load, screen, test } from "@snapfire/fsr-client/testing";

function services() {
  return { fleet: { listAgents: () => [], listAlerts: () => [] } };
}

test("t reads the catalog of the document's locale, with plural forms and placeholders", async () => {
  const c = ctx({ session: { watching: {}, density: "comfortable" }, services: services() });
  await load("/fr_FR/help", { ctx: c });
  assert.ok(screen.getByText("Comment ça marche"));
  assert.ok(catalog("fr_FR"), "the document embedded the French table");
  assert.equal(catalog("fr_FR")?.["help.title"], "Comment ça marche");
  assert.equal(t("help.title"), "Comment ça marche");
  assert.equal(t("agents.watching", { count: 1 }), "1 agent suivi");
  assert.equal(t("agents.watching", { count: 3 }), "3 agents suivis");
  assert.equal(t("nothing.here"), "nothing.here");

  await load("/help", { ctx: c });
  assert.ok(screen.getByText("How this works"));
  assert.equal(t("agents.watching", { count: 1 }), "watching 1 agent");
  setLocale("fr_FR");
  assert.equal(t("help.title"), "Comment ça marche", "the French table is still held after switching back");
});
