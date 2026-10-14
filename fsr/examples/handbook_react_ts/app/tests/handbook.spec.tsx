import { expect, fireEvent, load, screen, test } from "@snapfire/fsr-client/testing";

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

test("an MDX page reads its loader's data as props and highlights its code", async () => {
  await load("/install");
  expect(document.title).toEqual("Install · The FSR handbook");
  expect(document.querySelectorAll("ol.steps li").length, "one item per step the loader returned").toEqual(4);
  expect(document.querySelector(".prose p:nth-of-type(2)")?.textContent).toContain("is 4 steps from");
  const figure = document.querySelector("[data-rehype-pretty-code-figure]");
  expect(figure?.querySelector("figcaption")?.textContent).toEqual("terminal");
  expect(figure?.querySelectorAll("[data-line]").length).toEqual(3);
  expect(figure?.querySelector("[data-highlighted-line]")?.textContent).toEqual("cargo install snapfire_fsr_cli");
});

test("a terminal session marks its prompt and its output apart", async () => {
  await load("/install");
  const session = document.querySelectorAll("[data-rehype-pretty-code-figure]")[1];
  expect(session?.querySelector("pre")?.getAttribute("data-language")).toEqual("shell-session");
  expect(session?.querySelector(".hl-prompt .hl-punctuation")?.textContent).toEqual("$");
  expect(Array.from(session?.querySelectorAll(".hl-output") ?? []).map((o) => o.textContent)).toEqual(["routes    /install               routes/install", "plan      generated/plan.sexp    18.7 KB written"]);
});

test("a markdown page takes its title from frontmatter and its headings from the map", async () => {
  await load("/faq");
  expect(document.title).toEqual("FAQ · The FSR handbook");
  expect(document.querySelector(".page.prose"), "the map's wrapper holds the page").toBeTruthy();
  expect(document.querySelector("h2.qa a")?.getAttribute("href")).toEqual("#does-this-site-run-a-server");
  expect(document.querySelectorAll("tbody tr").length).toEqual(4);
});

test("an MDX page's own layout stands in for the wrapper", async () => {
  await load("/tips");
  expect(document.title).toEqual("Tips · The FSR handbook");
  expect(document.querySelector("section.page.tips h1")?.textContent).toEqual("Tips");
  expect(document.querySelector(".page.prose"), "no wrapper around a page with a layout").toBeFalsy();
  expect(document.querySelectorAll("ul.contains-task-list input[checked]").length).toEqual(2);
  expect(document.querySelector("section[data-footnotes] li")?.textContent).toContain("which the bundle compiles from the module");
});

test("an island renders an imported MDX file in the browser", async () => {
  await load("/tips");
  expect(document.querySelector("[data-part=tip] h2"), "closed on the server").toBeFalsy();
  await fireEvent.click(screen.getByText("Show the tip"));
  expect(document.querySelector("[data-part=tip] strong")?.textContent).toEqual("Tip:");
  expect(document.querySelector("[data-part=tip] h2.qa a")?.getAttribute("href"), "the browser reads the same map").toEqual("#from-the-browser");
});
