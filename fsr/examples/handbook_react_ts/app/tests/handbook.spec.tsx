import { assert, load, test } from "@snapfire/fsr-client/testing";

test("the home page renders its cards and links into the site", async () => {
  await load("/");
  assert.equal(document.querySelector("h1")?.textContent, "A site with no server");
  const titles = Array.from(document.querySelectorAll(".card h2")).map((h) => h.textContent);
  assert.equal(titles.length, 3, `three cards from the loader's constant, got ${titles.join(", ")}`);
  const links = Array.from(document.querySelectorAll("a")).map((a) => a.getAttribute("href"));
  assert.ok(links.includes("/install"), `the layout and the page link on, got ${links.join(" ")}`);
});

test("every page is wrapped by the layout", async () => {
  await load("/faq");
  assert.ok(document.querySelector(".wordmark"), "the masthead is there");
  assert.equal(document.querySelectorAll(".qa").length, 3, "one entry per question");
});
