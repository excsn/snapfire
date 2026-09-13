import { assert, ctx, load, test } from "@snapfire/fsr-client/testing";

const trimmer = { id: "1", name: "Hedge trimmer", category: "Garden", keeper: "Dev", deposit: 20, days: 3, note: "" };
const shed = { name: "The Shed", strap: "A street's worth of tools", categories: ["Garden"] };

test("reserving through the action and asking for the page again as a fragment shows the reservation", async () => {
  const c = ctx({
    session: { reserved: {} },
    services: { shed: { getShed: () => shed, listTools: () => [trimmer], listLoans: () => [], getWeather: () => ({ day: "Saturday", summary: "dry" }) } },
  });
  await load("/tool/1", { ctx: c });
  assert.ok(document.querySelector(".reserve .btn")?.textContent?.includes("Reserve it"));

  const posted = await fetch("/_sf/action/tool.$id.reserve", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ tool_id: "1", days: 2 }) });
  assert.equal(posted.status, 200);
  assert.equal(c.session.reserved, { "1": 2 }, "the action wrote the agreed length through the interpreter");

  const html = await (await fetch("/tool/1?__fragment")).text();
  assert.ok(html.includes("Let it go") && html.includes("Reserved for you"), "the fragment is rendered from the session the action wrote");
  assert.ok(html.includes("tool.$id.release"), "and posts the other action now");
  assert.ok(html.includes('"shed/reserved":1') || html.includes("shed/reserved"), "the seed carries the new count for the tally");
});

test("the loan slider moves until the tool is reserved, then it is settled", async () => {
  const c = ctx({
    session: { reserved: {} },
    services: { shed: { getShed: () => shed, listTools: () => [trimmer], listLoans: () => [], getWeather: () => ({ day: "Saturday", summary: "dry" }) } },
  });
  await load("/tool/1", { ctx: c });

  const free = await (await fetch("/tool/1?__fragment")).text();
  assert.ok(free.includes("Borrow for") && !free.includes('disabled=""'), "the slider moves while nobody has it");
  assert.ok(free.includes('max="3"'), "and it stops at what the shed lends it for");

  await fetch("/_sf/action/tool.$id.reserve", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ tool_id: "1", days: 2 }) });

  const held = await (await fetch("/tool/1?__fragment")).text();
  assert.ok(held.includes("Borrowed for") && held.includes('disabled=""'), "the length is settled once it is reserved");
  assert.ok(held.includes('value="2"'), "and it sits at the length that was asked for, not the shed's limit");
  assert.ok(!held.includes("<loan-planner name="), "with no name on the host, so the release post carries nothing from it");
});

test("a form post carries the slider's length as a field and lands back on the page", async () => {
  const c = ctx({
    session: { reserved: {} },
    services: { shed: { getShed: () => shed, listTools: () => [trimmer], listLoans: () => [], getWeather: () => ({ day: "Saturday", summary: "dry" }) } },
  });
  await load("/tool/1", { ctx: c });

  const posted = await fetch("/_sf/action/tool.$id.reserve?__fragment", {
    method: "POST",
    headers: { "content-type": "application/x-www-form-urlencoded", referer: "/tool/1" },
    body: "_csrf=t&tool_id=1&days=2",
  });
  assert.equal(posted.status, 303, "a form post is answered with the redirect back");
  assert.equal(posted.headers.get("location"), "/tool/1?__fragment", "carrying the fragment the action was asked for");
  assert.equal(c.session.reserved, { "1": 2 }, "and `days` arrived as a number the action could clamp");

  const refused = await fetch("/_sf/action/tool.$id.reserve", {
    method: "POST",
    headers: { "content-type": "application/x-www-form-urlencoded" },
    body: "_csrf=t&tool_id=1&days=soon",
  });
  assert.equal(refused.status, 400, "a field that is not the declared type is refused");
});
