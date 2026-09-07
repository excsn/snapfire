import { assert, ctx, fireEvent, load, test } from "@snapfire/fsr-client/testing";

const wave = {
  id: "kickoff",
  title: "Snapfire kickoff",
  participants: ["alice", "bob"],
  blips: [
    { id: "1", parent: "", who: "alice", body: "Starting a wave.", at: "09:10", edited: "", editors: [], depth: 0 },
    { id: "2", parent: "1", who: "bob", body: "Under the first.", at: "09:12", edited: "09:20", editors: ["alice", "carol"], depth: 1 },
  ],
};

const open = (name: string) =>
  ctx({
    session: { name, waves: {} },
    services: {
      waves: {
        getWave: () => wave,
        listWaves: () => [
          { id: "kickoff", title: "Snapfire kickoff", participants: ["alice", "bob"], blips: 2, last: "09:12" },
          { id: "board", title: "Arrivals board review", participants: ["alice"], blips: 1, last: "08:02" },
        ],
        listPeople: () => [
          { name: "alice", here: true, waves: 2 },
          { name: "bob", here: false, waves: 1 },
        ],
      },
    },
  });

test("the wave renders every blip at the depth the service gave it", async () => {
  await load("/wave/kickoff", { ctx: open("bob") });
  const items = Array.from(document.querySelectorAll(".blips > li"));
  assert.equal(items.length, 2, "one item per blip");
  assert.equal(items[0]?.getAttribute("style"), "margin-left:0rem", "a top level blip is flush");
  assert.equal(items[1]?.getAttribute("style"), "margin-left:1.5rem", "a reply is indented one step");
  assert.equal(items[1]?.querySelector(".blip")?.className, "blip mine", "bob's own blip is marked");
});

test("the live parts are islands and the transcript is not", async () => {
  await load("/wave/kickoff", { ctx: open("alice") });
  assert.ok(document.querySelector(".wave-head .live"), "the connection pill is placed");
  assert.equal(document.querySelectorAll(".people .person").length, 2, "a participant each");
  assert.equal(document.querySelectorAll(".under").length, 3, "one under each blip and one for the wave");
  assert.equal(document.querySelectorAll(".composer").length, 1, "only the wave's own composer is open to begin with");
  assert.equal(document.querySelectorAll(".blip.ghost").length, 0, "nobody is typing in a spec");
});

test("every blip offers itself for rewriting and says when it was", async () => {
  await load("/wave/kickoff", { ctx: open("alice") });
  assert.equal(document.querySelectorAll(".blips .take").length, 2, "a blip is a document, so each one can be taken");
  const stamps = Array.from(document.querySelectorAll(".blips .edited")).map((s) => s.textContent);
  assert.equal(stamps, ["edited 09:20"], "only the blip that was amended says so");
  const editors = Array.from(document.querySelectorAll(".blips .editor")).map((s) => s.textContent);
  assert.equal(editors, ["alice", "carol"], "and it names everyone who has rewritten it");
  assert.equal(document.querySelectorAll(".blips .rewrite").length, 0, "and nobody is rewriting one in a spec");
});

test("a reader with no name is offered nothing to write with", async () => {
  await load("/wave/kickoff", { ctx: open("") });
  assert.equal(document.querySelectorAll(".composer").length, 0, "no composer");
  assert.equal(document.querySelectorAll(".take").length, 0, "and no blip offers itself for rewriting");
  assert.equal(document.querySelectorAll(".nameless").length, 1, "the wave says what to do about it, once");
});

test("every island region names itself, so a re-render pairs a placement with its own", async () => {
  await load("/wave/kickoff", { ctx: open("alice") });
  const keys = Array.from(document.querySelectorAll("sf-s[data-sf-island]")).map((s) => s.getAttribute("data-sf-region"));
  assert.equal(keys.length, new Set(keys).size, `a region key is unique in a document, got ${keys.join(" ")}`);
  const page = keys.filter((k) => k?.startsWith("routes/wave/[id]/page.tsx#default|"));
  assert.equal(page.length, 6, "the wave places six islands: presence, the wave composer, and a body and an under for each blip");
  assert.ok(page.includes("routes/wave/[id]/page.tsx#default|i1@1"), `a placement in a loop keys under its iteration, got ${page.join(" ")}`);
});

test("the inbox lists every wave beside the open one", async () => {
  await load("/wave/kickoff", { ctx: open("alice") });
  const hrefs = Array.from(document.querySelectorAll(".wave-list a")).map((a) => a.getAttribute("href"));
  assert.ok(hrefs.includes("/wave/kickoff"), `the list is a slot of the layout, got ${hrefs.join(" ")}`);
});

test("the inbox marks the wave the request is on", async () => {
  await load("/wave/board", { ctx: open("alice") });
  const open_ = Array.from(document.querySelectorAll(".wave-card.open")).map((a) => a.getAttribute("href"));
  assert.equal(open_, ["/wave/board"], "the card for the open wave is the one marked");
});

test("the rail keeps the wave under every view", async () => {
  await load("/wave/kickoff", { ctx: open("alice") });
  const hrefs = Array.from(document.querySelectorAll(".rail a")).map((a) => a.getAttribute("href"));
  assert.equal(hrefs, ["/wave/kickoff?view=inbox", "/wave/kickoff?view=active", "/wave/kickoff?view=mine"], "each view is this page with a different query");
  assert.equal(document.querySelectorAll(".view.on").length, 1, "one view is current");
});

test("the contacts pane shows presence across every wave", async () => {
  await load("/wave/kickoff", { ctx: open("alice") });
  const names = Array.from(document.querySelectorAll(".contact")).map((li) => li.className);
  assert.equal(names, ["contact on", "contact"], "alice is here and bob is not");
});

/// DEFECTS 5.3: the client action path, and the revalidation it triggers.
test("keeping a blip calls the action and the transcript follows without a reload", async () => {
  const blips = [{ id: "1", parent: "", who: "alice", body: "Starting a wave.", at: "09:10", edited: "", editors: [], depth: 0 }];
  const live = ctx({
    session: { name: "dora", waves: {} },
    services: {
      waves: {
        getWave: () => ({ id: "kickoff", title: "Snapfire kickoff", participants: ["alice"], blips }),
        addBlip: (input: { body: string }) => {
          const kept = { id: String(blips.length + 1), parent: "", who: "dora", body: input.body, at: "09:30", edited: "", editors: [], depth: 0 };
          blips.push(kept);
          return kept;
        },
        listWaves: () => [{ id: "kickoff", title: "Snapfire kickoff", participants: ["alice"], blips: blips.length, last: "09:30" }],
        listPeople: () => [{ name: "alice", here: true, waves: 1 }],
      },
    },
  });

  await load("/wave/kickoff", { ctx: live });
  assert.equal(document.querySelectorAll(".blips > li").length, 1, "one blip to begin with");

  const composer = document.querySelector(".composer input") as HTMLInputElement;
  await fireEvent.change(composer, "written through the action client");
  await fireEvent.submit(composer);

  const bodies = Array.from(document.querySelectorAll(".blips .body")).map((b) => b.textContent);
  assert.equal(bodies, ["Starting a wave.", "written through the action client"], `the action ran and the page revalidated in place, got ${bodies.join(" | ")}`);
  assert.equal(document.querySelectorAll(".blips > li").length, 2, "and the new blip is one item, in its own region");
});
