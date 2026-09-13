import { key, set } from "@snapfire/fsr-client/store";
import { assert, ctx, fireEvent, load, settle, test } from "@snapfire/fsr-client/testing";

const board = { cells: Array.from({ length: 9 }, (_, at) => ({ at, mark: "" })), turn: "x", won: "" };

interface Part {
  kind: string;
  text: string;
  href: string;
  at: string;
  children: Part[];
  replies: Blip[];
}

interface Blip {
  id: string;
  parent: string;
  anchor: string;
  who: string;
  body: string;
  at: string;
  edited: string;
  editors: string[];
  parts: Part[];
  replies: Blip[];
}

const text = (words: string): Part => ({ kind: "text", text: words, href: "", at: "", children: [], replies: [] });
const strong = (words: string): Part => ({ kind: "strong", text: "", href: "", at: "", children: [text(words)], replies: [] });
const para = (children: Part[], replies: Blip[] = []): Part => ({ kind: "p", text: "", href: "", at: "0", children, replies });
const blip = (id: string, parent: string, anchor: string, who: string, words: string, more: Partial<Blip> = {}): Blip => ({
  id,
  parent,
  anchor,
  who,
  body: words,
  at: `09:1${id}`,
  edited: "",
  editors: [],
  parts: [para([text(words)])],
  replies: [],
  ...more,
});

/** Alice's blip with bob's reply under it, alice's reply to that and carol's reply beside the one paragraph of the first. An island in the reply's reply and one in the reply beside the paragraph sit at the same loop indices, which is what their region keys have to tell apart. */
const wave = {
  id: "kickoff",
  title: "Snapfire kickoff",
  participants: ["alice", "bob"],
  game: board,
  blips: [
    blip("1", "", "", "alice", "Starting a wave.", {
      body: "Starting **a** wave.",
      parts: [para([text("Starting "), strong("a"), text(" wave.")], [blip("3", "1", "0", "carol", "Beside the first.")])],
      replies: [blip("2", "1", "", "bob", "Under the first.", { edited: "09:20", editors: ["alice", "carol"], replies: [blip("4", "2", "", "alice", "Under the second.")] })],
    }),
  ],
};

const listings = {
  listWaves: () => [
    { id: "kickoff", title: "Snapfire kickoff", participants: ["alice", "bob"], blips: 4, last: "09:14" },
    { id: "board", title: "Arrivals board review", participants: ["alice"], blips: 1, last: "08:02" },
  ],
  listPeople: () => [
    { name: "alice", here: true, waves: 2 },
    { name: "bob", here: false, waves: 1 },
  ],
};

const open = (name: string) => ctx({ session: { name, waves: {} }, services: { waves: { getWave: () => wave, ...listings } } });

/** The first blip's own paragraph, whose aside holds carol's reply and an `Under` of its own. */
const FIRST_BLOCK = ".blips > .thread > .blip > .body.md > .block";

test("a reply sits inside the blip it answers and a reply to a block sits under that block", async () => {
  await load("/wave/kickoff", { ctx: open("bob") });
  assert.equal(document.querySelectorAll(".blips > .thread").length, 1, "one blip at the top");
  assert.equal(document.querySelectorAll(".blips > .thread > .replies > .thread").length, 1, "bob's reply is inside alice's blip");
  assert.equal(document.querySelectorAll(".replies .replies > .thread").length, 1, "and alice's reply to bob inside his");
  assert.equal(document.querySelector(".blips > .thread > .replies > .thread > .blip")?.className, "blip mine", "bob's own blip is marked");
  const beside = Array.from(document.querySelectorAll(`${FIRST_BLOCK} > .asides > .thread > .blip > .who`)).map((who) => who.textContent);
  assert.equal(beside, ["carol"], "carol's reply sits under the paragraph she answered");
});

test("the live parts are islands and the transcript is not", async () => {
  await load("/wave/kickoff", { ctx: open("alice") });
  assert.ok(document.querySelector(".wave-head .live"), "the connection pill is placed");
  assert.equal(document.querySelectorAll(".people .person").length, 2, "a participant each");
  assert.equal(document.querySelectorAll(".under").length, 9, "one under each blip, one beside each paragraph and one for the wave");
  assert.equal(document.querySelectorAll(".under.aside").length, 4, "four blips of one paragraph each");
  assert.equal(document.querySelectorAll(".composer").length, 1, "only the wave's own composer is open to begin with");
  assert.equal(document.querySelectorAll(".blip.ghost").length, 0, "nobody is typing in a spec");
});

test("every blip offers itself for rewriting and says when it was", async () => {
  await load("/wave/kickoff", { ctx: open("alice") });
  assert.equal(document.querySelectorAll(".blips .take").length, 4, "a blip is a document, so each one can be taken");
  const stamps = Array.from(document.querySelectorAll(".blips .edited")).map((s) => s.textContent);
  assert.equal(stamps, ["edited 09:20"], "only the blip that was amended says so");
  const editors = Array.from(document.querySelectorAll(".blips .editor")).map((s) => s.textContent);
  assert.equal(editors, ["alice", "carol"], "and it names everyone who has rewritten it");
  assert.equal(document.querySelectorAll(".blips .rewrite").length, 0, "and nobody is rewriting one in a spec");
});

test("a reader with no name is offered nothing to write with", async () => {
  await load("/wave/kickoff", { ctx: open("") });
  assert.equal(document.querySelectorAll(".composer").length, 0, "no composer");
  assert.equal(document.querySelectorAll(".take").length, 0, "no blip offers itself for rewriting");
  assert.equal(document.querySelectorAll(".reply").length, 0, "and neither a blip nor a paragraph offers a reply");
  assert.equal(document.querySelectorAll(".nameless").length, 1, "the wave says what to do about it, once");
});

test("every island region names itself, so a re-render pairs a placement with its own", async () => {
  await load("/wave/kickoff", { ctx: open("alice") });
  const keys = Array.from(document.querySelectorAll("sf-s[data-sf-island]")).map((s) => s.getAttribute("data-sf-region") ?? "");
  assert.equal(keys.length, new Set(keys).size, `a region key is unique in a document, got ${keys.join(" ")}`);
  assert.equal(keys.filter((k) => k.startsWith("routes/wave/[id]/page.tsx#default|")).length, 3, "the page places presence, the gadget and the wave's composer");
  const blips = keys.filter((k) => k.startsWith("src/ui/Blip.tsx#"));
  assert.equal(blips.length, 12, "a body and an under for each of four blips and an under beside each of their paragraphs");
  assert.ok(blips.some((k) => /@0\.c\d+$/.test(k)), `a blip's islands key under the page's loop and the placement that rendered it, got ${blips.join(" ")}`);
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

test("a blip's body is its parts rendered as elements", async () => {
  await load("/wave/kickoff", { ctx: open("alice") });
  const first = document.querySelector(`${FIRST_BLOCK} > p`);
  assert.equal(first?.querySelector("strong")?.textContent, "a", "the markdown arrived as elements rather than as asterisks");
  assert.equal(first?.textContent, "Starting a wave.", "with its text around them");
});

test("a window's own rewrite never takes its blip away from it", async () => {
  await load("/wave/kickoff", { ctx: open("alice") });
  const edits = key<{ blip: string; who: string; body: string }[]>("wave/edits");
  set(edits, [{ blip: "1", who: "alice", body: "mine" }]);
  await settle();
  assert.equal(document.querySelectorAll(".blips .body-held").length, 0, "alice's own rewrite, which a uniform view carries, is not shown to her as someone else's");
  set(edits, [{ blip: "1", who: "bob", body: "his" }]);
  await settle();
  assert.equal(document.querySelectorAll(".blips .body-held").length, 1, "bob's rewrite still holds the blip in alice's window");
});

test("someone typing beside a block shows there and nowhere else", async () => {
  await load("/wave/kickoff", { ctx: open("alice") });
  set(key<{ who: string; parent: string; anchor: string; body: string }[]>("wave/drafts"), [{ who: "bob", parent: "1", anchor: "0", body: "beside" }]);
  await settle();
  assert.equal(document.querySelectorAll(".blip.ghost").length, 1, "one ghost");
  assert.equal(document.querySelectorAll(`${FIRST_BLOCK} > sf-s .blip.ghost`).length, 1, "beside the paragraph bob is answering");
});

test("a reply to a block goes to the action with the block it answers", async () => {
  const asked: { parent: string; anchor: string; body: string }[] = [];
  const live = ctx({
    session: { name: "dora", waves: {} },
    services: {
      waves: {
        getWave: () => wave,
        addBlip: (input: { parent: string; anchor: string; body: string }) => {
          asked.push({ parent: input.parent, anchor: input.anchor, body: input.body });
          return blip("9", input.parent, input.anchor, "dora", input.body);
        },
        ...listings,
      },
    },
  });
  await load("/wave/kickoff", { ctx: live });
  await fireEvent.click(document.querySelector(`${FIRST_BLOCK} > sf-s .reply`) as Element);
  const input = document.querySelector(`${FIRST_BLOCK} > sf-s .composer input`) as HTMLInputElement;
  await fireEvent.change(input, "a word on that");
  await fireEvent.submit(input);
  assert.equal(asked, [{ parent: "1", anchor: "0", body: "a word on that" }], "the action carried the paragraph's path");
});

/// DEFECTS 5.3: the client action path and the revalidation it triggers.
test("keeping a blip calls the action and the transcript follows without a reload", async () => {
  const blips = [blip("1", "", "", "alice", "Starting a wave.")];
  const live = ctx({
    session: { name: "dora", waves: {} },
    services: {
      waves: {
        getWave: () => ({ id: "kickoff", title: "Snapfire kickoff", participants: ["alice"], game: board, blips }),
        addBlip: (input: { body: string }) => {
          const kept = blip(String(blips.length + 1), "", "", "dora", input.body);
          blips.push(kept);
          return kept;
        },
        listWaves: () => [{ id: "kickoff", title: "Snapfire kickoff", participants: ["alice"], blips: blips.length, last: "09:30" }],
        listPeople: () => [{ name: "alice", here: true, waves: 1 }],
      },
    },
  });

  await load("/wave/kickoff", { ctx: live });
  assert.equal(document.querySelectorAll(".blips > .thread").length, 1, "one blip to begin with");

  const composer = document.querySelector(".wave > sf-s .composer input") as HTMLInputElement;
  await fireEvent.change(composer, "written through the action client");
  await fireEvent.submit(composer);

  const bodies = Array.from(document.querySelectorAll(".blips .body.md > .block > p")).map((b) => b.textContent?.trim());
  assert.equal(bodies, ["Starting a wave.", "written through the action client"], `the action ran and the page revalidated in place, got ${bodies.join(" | ")}`);
  assert.equal(document.querySelectorAll(".blips > .thread").length, 2, "and the new blip is one item, in its own region");
});
