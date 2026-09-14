import { ctx, expect, load, test } from "@snapfire/fsr-client/testing";

const trimmer = { id: "1", name: "Hedge trimmer", category: "Garden", keeper: "Dev", deposit: 20, days: 3, note: "" };
const drill = { ...trimmer, id: "3", name: "Cordless drill", category: "Workshop", keeper: "Priya" };
const shed = { name: "The Shed", strap: "A street's worth of tools", categories: ["Garden", "Workshop"] };
const loans = [{ tool: "Gazebo", who: "the Okafors", back: "2026-09-11" }];

const stocked = () =>
  ctx({
    services: { shed: { getShed: () => shed, listTools: () => [trimmer, drill], listLoans: () => loans, getWeather: () => ({ day: "Saturday", summary: "dry" }) } },
  });

test("the page as a fragment is the page's markup and the route's seed, nothing else", async () => {
  await load("/", { ctx: stocked() });
  const response = await fetch("/?category=Workshop&__fragment");
  expect(response.status).toEqual(200);
  const html = await response.text();
  expect(html.startsWith("<section"), `bare markup: ${html.slice(0, 40)}`).toBeTruthy();
  expect(html.includes("Cordless drill") && !html.includes("Hedge trimmer"), "filtered by the query the fragment carried").toBeTruthy();
  expect(!html.includes("masthead") && !html.includes("<html"), "no layout and no shell").toBeTruthy();
  expect(!html.includes("<!--sf-g:"), "no segment delimiters").toBeTruthy();
  expect(html.includes("data-sf-store") && html.includes("shed/reserved"), "the seed follows the markup").toBeTruthy();
});

test("a slot as a fragment is that slot alone, its loader run", async () => {
  await load("/tool/1", { ctx: stocked() });
  const response = await fetch("/tool/1?__fragment=loans");
  const html = await response.text();
  expect(html.startsWith('<div class="panel loans"'), html.slice(0, 60)).toBeTruthy();
  expect(html.includes("the Okafors")).toBeTruthy();
  expect(!html.includes("Hedge trimmer"), "the page is not in it").toBeTruthy();
});

test("a slot the route does not have is not found", async () => {
  await load("/", { ctx: stocked() });
  const response = await fetch("/?__fragment=nope");
  expect(response.status).toEqual(404);
});
