import { expect, load, test } from "@snapfire/fsr-client/testing";

test("the index lists every post with a literal link under the prefix", async () => {
  await load("/blog");
  const links = Array.from(document.querySelectorAll("[data-part=posts] a")).map((a) => a.getAttribute("href"));
  expect(links).toContain("/blog/post/why-a-plan-file");
  expect(document.title).toEqual("Posts · Blog");
});

test("a post page renders the body the application produced", async () => {
  await load("/blog/post/static-under-a-shell");
  expect(document.querySelector("[data-part=body] code")?.textContent).toEqual("fsr prerender");
  expect(document.title).toEqual("Static pages under a shell that reads the session · Blog");
});

test("the tag list groups posts by tag", async () => {
  await load("/blog/tags");
  const groups = Array.from(document.querySelectorAll("[data-part=tag] h2")).map((h) => h.textContent);
  expect(groups).toEqual(["design", "prerender", "runtime", "sites"]);
});

test("a slug off the blog answers 404 with the blog's error page", async () => {
  const { status } = await load("/blog/post/nope");
  expect(status).toEqual(404);
  expect(document.querySelector("h1")?.textContent).toEqual("That did not load");
});


test("an MDX post renders its markdown, its JSX and the components the map names", async () => {
  await load("/blog/post/why-a-plan-file");
  const body = document.querySelector("[data-part=body]");
  expect(body?.querySelector("h2#what-a-loader-becomes a")?.getAttribute("href")).toEqual("#what-a-loader-becomes");
  expect(body?.querySelector("[data-part=aside]")?.textContent).toEqual("TagsThis post carries 2 tags: design and runtime.");
  expect(Array.from(body?.querySelectorAll("th") ?? []).map((th) => th.textContent)).toEqual(["Part of the plan", "Read by"]);
  expect(body?.querySelector("sup a[data-footnote-ref]")?.getAttribute("href")).toEqual("#user-content-fn-rust");
  expect(body?.querySelector("section[data-footnotes] li")?.textContent).toContain("no JavaScript engine on the request path");
  const toc = Array.from(document.querySelectorAll("[data-part=toc] a")).map((a) => a.getAttribute("href"));
  expect(toc).toEqual(["#what-a-loader-becomes", "#who-reads-which-part"]);
});

test("a code block is highlighted at build with its title and marked line", async () => {
  await load("/blog/post/why-a-plan-file");
  const figure = document.querySelector("[data-rehype-pretty-code-figure]");
  expect(figure?.querySelector("figcaption")?.textContent).toEqual("routes/post/[slug]/page.loader.ts");
  expect(figure?.querySelector("pre")?.className).toEqual("hl-code");
  const lines = Array.from(figure?.querySelectorAll("[data-line]") ?? []);
  expect(lines.length).toEqual(3);
  expect(lines[1].hasAttribute("data-highlighted-line")).toEqual(true);
  expect(figure?.querySelector(".hl-entity.hl-name.hl-function")?.textContent).toEqual("load");
});

test("a components prop places markdown links as Link, which marks the current page", async () => {
  await load("/blog/post/a-site-is-a-mount");
  const body = document.querySelector("[data-part=body]");
  expect(body?.querySelector("a[href='/blog/post/a-site-is-a-mount']")?.getAttribute("aria-current")).toEqual("page");
  expect(body?.querySelector("a[href='/blog/tags']")?.getAttribute("aria-current")).toEqual(null);
  expect(body?.querySelector("blockquote")?.className).toContain("border-l-4");
  expect(Array.from(body?.querySelectorAll("li input[type=checkbox]") ?? []).map((box) => box.hasAttribute("checked"))).toEqual([true, true, false]);
  expect(body?.querySelector("[data-part=claps]")?.textContent).toEqual("Clap 0");
});

test("a markdown post takes its meta from frontmatter", async () => {
  await load("/blog/post/static-under-a-shell");
  expect(document.querySelector("article h1")?.textContent).toEqual("Static pages under a shell that reads the session");
  expect(document.querySelector("[data-part=body] del")?.textContent).toEqual("per request");
  expect(document.querySelectorAll("[data-part=body] ol li").length).toEqual(2);
});

test("the tag list shows each post's first paragraph from the collection", async () => {
  await load("/blog/tags");
  const item = document.querySelector("[data-part=tag] a[href='/blog/post/a-site-is-a-mount']")?.closest("li");
  expect(item?.querySelector("[data-part=excerpt]")?.textContent?.startsWith("The shell reads the site's artifact from a path")).toEqual(true);
  expect(document.querySelectorAll("[data-part=tag] li").length).toEqual(document.querySelectorAll("[data-part=tag] [data-part=excerpt]").length);
});
