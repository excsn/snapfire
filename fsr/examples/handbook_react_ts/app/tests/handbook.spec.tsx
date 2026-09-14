import { expect, load, test } from "@snapfire/fsr-client/testing";

test("the home page renders its cards and links into the site", async () => {
  await load("/");
  expect(document.querySelector("h1")?.textContent).toEqual("A site with no server");
  const titles = Array.from(document.querySelectorAll(".card h2")).map((h) => h.textContent);
  expect(titles.length, `three cards from the loader's constant, got ${titles.join(", ")}`).toEqual(3);
  const links = Array.from(document.querySelectorAll("a")).map((a) => a.getAttribute("href"));
  expect(links.includes("/install"), `the layout and the page link on, got ${links.join(" ")}`).toBeTruthy();
});

test("every page is wrapped by the layout", async () => {
  await load("/faq");
  expect(document.querySelector(".wordmark"), "the masthead is there").toBeTruthy();
  expect(document.querySelectorAll(".qa").length, "one entry per question").toEqual(3);
});
