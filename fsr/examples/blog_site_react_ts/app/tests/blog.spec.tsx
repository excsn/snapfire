import { expect, load, test } from "@snapfire/fsr-client/testing";

test("the index lists every post with a literal link under the prefix", async () => {
  await load("/blog");
  const links = Array.from(document.querySelectorAll(".post-list a")).map((a) => a.getAttribute("href"));
  expect(links).toContain("/blog/post/why-a-plan-file");
  expect(document.title).toEqual("Posts · Blog");
});

test("a post page renders the body the application produced", async () => {
  await load("/blog/post/static-under-a-shell");
  expect(document.querySelector(".post-body code")?.textContent).toEqual("fsr prerender");
  expect(document.title).toEqual("Static pages under a shell that reads the session · Blog");
});

test("the tag list groups posts by tag", async () => {
  await load("/blog/tags");
  const groups = Array.from(document.querySelectorAll(".tag-group h2")).map((h) => h.textContent);
  expect(groups).toEqual(["design", "prerender", "runtime", "sites"]);
});

test("a slug off the blog answers 404 with the blog's error page", async () => {
  const { status } = await load("/blog/post/nope");
  expect(status).toEqual(404);
  expect(document.querySelector("h1")?.textContent).toEqual("That did not load");
});

