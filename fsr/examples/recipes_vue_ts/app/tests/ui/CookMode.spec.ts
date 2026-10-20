import { expect, fireEvent, render, settle, test } from "@snapfire/fsr-client/testing";

import CookMode from "@src/ui/CookMode.vue";

test("cook mode's dialog is written at the end of the body and opens over the page", async () => {
  const r = await render({ type: CookMode, props: { title: "Leek and potato soup", method: "Sweat the leeks." } });
  const dialog = document.querySelector("dialog.cook");
  expect(dialog?.parentElement, "the server placed it in the body, not in the island").toBe(document.body);
  expect(dialog?.hasAttribute("open"), "closed until asked").toEqual(false);
  expect(r.container.querySelector("dialog"), "nothing is left in place").toBe(null);

  await fireEvent.click(r.getByText("Cook mode"));
  await settle();
  expect(document.querySelector("dialog.cook"), "Vue hydrated the server's dialog rather than mounting a second").toBe(dialog);
  expect(dialog?.hasAttribute("open")).toEqual(true);
});
