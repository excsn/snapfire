import { createEvent, expect, fireEvent, load, screen, settle, test } from "@snapfire/fsr-client/testing";

/** Clicks `link` without settling, so the navigation it starts is still in flight when the caller looks. */
async function startFollowing(link: Element): Promise<void> {
  link.dispatchEvent(createEvent("click", link));
  for (let i = 0; i < 5; i++) await Promise.resolve();
}

test("a Svelte island the server placed in a link reads that link's navigation as pending", async () => {
  await load("/");
  await settle();
  const link = document.querySelector("a.to-stats");
  expect(link?.querySelector(".opening"), "the link holds the island").toBeTruthy();
  await startFollowing(link as Element);
  expect(link?.hasAttribute("data-sf-pending"), "the link is marked while the payload is fetched").toBe(true);
  expect(link?.querySelector(".opening-on")?.textContent, "and the island inside it says so").toEqual("opening…");
  await settle();
  expect(location.pathname).toEqual("/stats");
  expect(document.querySelector("[data-sf-pending]"), "the mark is gone once the page is shown").toBeNull();
});

test("a component inside FSR's Link in a Svelte island reads that link's navigation as pending", async () => {
  await load("/");
  await settle();
  await fireEvent.click(screen.getByText("Piranesi"));
  await settle();
  const link = document.querySelector(".recent li a");
  expect(link?.querySelector(".opening"), "the island's own link holds the component").toBeTruthy();
  await startFollowing(link as Element);
  expect(link?.querySelector(".opening-on")?.textContent).toEqual("opening…");
  await settle();
  expect(document.querySelector(".opening-on"), "nothing is pending once the page is shown").toBeNull();
});
