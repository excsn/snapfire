import { key, set } from "@snapfire/fsr-client/store";
import { ctx, expect, fireEvent, load, settle, spyOn, test } from "@snapfire/fsr-client/testing";

interface Gadget {
  kind: string;
  question: string;
  cells: { at: number; mark: string }[];
  turn: string;
  won: string;
  choices: { answer: string; count: number; who: string[] }[];
  closed: boolean;
  ended: boolean;
  ends: string;
  lit: boolean;
}

/** What a block that is no gadget carries. */
const none: Gadget = { kind: "", question: "", cells: [], turn: "", won: "", choices: [], closed: false, ended: false, ends: "", lit: false };
const noughts: Gadget = { ...none, kind: "noughts", cells: Array.from({ length: 9 }, (_, at) => ({ at, mark: "" })), turn: "x" };
const poll: Gadget = { ...none, kind: "poll", question: "When?", choices: [{ answer: "Thursday", count: 1, who: ["bob"] }, { answer: "Friday", count: 0, who: [] }] };

/** The wave as it stands after five changes, the last of them alice's fourth blip. */
const clock = { step: 5, steps: 5, live: true, change: { kind: "kept", who: "alice", at: "09:14", blip: "4" } };

interface Part {
  kind: string;
  text: string;
  href: string;
  at: string;
  children: Part[];
  replies: Blip[];
}

interface Block {
  id: string;
  text: string;
  parts: Part[];
  gadget: Gadget;
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
  blocks: Block[];
  replies: Blip[];
  lit: boolean;
}

const text = (words: string): Part => ({ kind: "text", text: words, href: "", at: "", children: [], replies: [] });
const strong = (words: string): Part => ({ kind: "strong", text: "", href: "", at: "", children: [text(words)], replies: [] });
const para = (at: string, children: Part[], replies: Blip[] = []): Part => ({ kind: "p", text: "", href: "", at, children, replies });
const item = (at: string, children: Part[], replies: Blip[] = []): Part => ({ kind: "li", text: "", href: "", at, children, replies });
const list = (items: Part[]): Part => ({ kind: "ul", text: "", href: "", at: "", children: items, replies: [] });
const gadgetPart = (at: string): Part => ({ kind: "gadget", text: "", href: "", at, children: [], replies: [] });
const blip = (id: string, parent: string, anchor: string, who: string, words: string, more: Partial<Blip> = {}): Blip => ({
  id,
  parent,
  anchor,
  who,
  body: words,
  at: `09:1${id}`,
  edited: "",
  editors: [],
  blocks: [{ id: "b1", text: words, parts: [para("b1", [text(words)])], gadget: none }],
  replies: [],
  lit: false,
  ...more,
});

/** Alice's blip with bob's reply under it, alice's reply to that and carol's reply beside the one paragraph of the first. An island in the reply's reply and one in the reply beside the paragraph sit at the same loop indices, which is what their region keys have to tell apart. */
const wave = {
  id: "kickoff",
  title: "Snapfire kickoff",
  participants: ["alice", "bob"],
  ...clock,
  blips: [
    blip("1", "", "", "alice", "Starting a wave.", {
      body: "Starting **a** wave.",
      blocks: [{ id: "b1", text: "Starting **a** wave.", parts: [para("b1", [text("Starting "), strong("a"), text(" wave.")], [blip("3", "1", "b1", "carol", "Beside the first.")])], gadget: none }],
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
  expect(document.querySelectorAll(".blips > .thread").length, "one blip at the top").toEqual(1);
  expect(document.querySelectorAll(".blips > .thread > .replies > .thread").length, "bob's reply is inside alice's blip").toEqual(1);
  expect(document.querySelectorAll(".replies .replies > .thread").length, "and alice's reply to bob inside his").toEqual(1);
  expect(document.querySelector(".blips > .thread > .replies > .thread > .blip")?.className, "bob's own blip is marked").toEqual("blip mine");
  const beside = Array.from(document.querySelectorAll(`${FIRST_BLOCK} > .asides > .thread > .blip > .who`)).map((who) => who.textContent);
  expect(beside, "carol's reply sits under the paragraph she answered").toEqual(["carol"]);
});

test("the live parts are islands and the transcript is not", async () => {
  await load("/wave/kickoff", { ctx: open("alice") });
  expect(document.querySelector(".wave-head .live"), "the connection pill is placed").toBeTruthy();
  expect(document.querySelectorAll(".people .person").length, "a participant each").toEqual(2);
  expect(document.querySelectorAll(".under").length, "one under each blip, one beside each paragraph and one for the wave").toEqual(9);
  expect(document.querySelectorAll(".under.aside").length, "four blips of one paragraph each").toEqual(4);
  expect(document.querySelectorAll(".composer").length, "only the wave's own composer is open to begin with").toEqual(1);
  expect(document.querySelectorAll(".blip.ghost").length, "nobody is typing in a spec").toEqual(0);
});

test("every blip offers itself for rewriting and says when it was", async () => {
  await load("/wave/kickoff", { ctx: open("alice") });
  expect(document.querySelectorAll(".blips .take").length, "a blip is a document, so each one can be taken").toEqual(4);
  const stamps = Array.from(document.querySelectorAll(".blips .edited")).map((s) => s.textContent);
  expect(stamps, "only the blip that was amended says so").toEqual(["edited 09:20"]);
  const editors = Array.from(document.querySelectorAll(".blips .editor")).map((s) => s.textContent);
  expect(editors, "and it names everyone who has rewritten it").toEqual(["alice", "carol"]);
  expect(document.querySelectorAll(".blips .rewrite").length, "and nobody is rewriting one in a spec").toEqual(0);
});

test("a reader with no name is offered nothing to write with", async () => {
  await load("/wave/kickoff", { ctx: open("") });
  expect(document.querySelectorAll(".composer").length, "no composer").toEqual(0);
  expect(document.querySelectorAll(".take").length, "no blip offers itself for rewriting").toEqual(0);
  expect(document.querySelectorAll(".edit-block").length, "and no block does").toEqual(0);
  expect(document.querySelectorAll(".reply").length, "and neither a blip nor a paragraph offers a reply").toEqual(0);
  expect(document.querySelectorAll(".nameless").length, "the wave says what to do about it, once").toEqual(1);
});

test("every island region names itself, so a re-render pairs a placement with its own", async () => {
  await load("/wave/kickoff", { ctx: open("alice") });
  const keys = Array.from(document.querySelectorAll("sf-s[data-sf-island]")).map((s) => s.getAttribute("data-sf-region") ?? "");
  expect(keys.length, `a region key is unique in a document, got ${keys.join(" ")}`).toEqual(new Set(keys).size);
  expect(keys.filter((k) => k.startsWith("routes/wave/[id]/page.tsx#default|")).length, "the page places presence, the scrubber and the wave's composer").toEqual(3);
  const blips = keys.filter((k) => k.startsWith("src/ui/Blip.tsx#"));
  expect(blips.length, "a body, an under and a block for each of four blips of one block each and an under beside each of their paragraphs").toEqual(16);
  expect(blips.some((k) => /@0\.c\d+$/.test(k)), `a blip's islands key under the page's loop and the placement that rendered it, got ${blips.join(" ")}`).toBeTruthy();
});

test("the inbox lists every wave beside the open one", async () => {
  await load("/wave/kickoff", { ctx: open("alice") });
  const hrefs = Array.from(document.querySelectorAll(".wave-list a")).map((a) => a.getAttribute("href"));
  expect(hrefs.includes("/wave/kickoff"), `the list is a slot of the layout, got ${hrefs.join(" ")}`).toBeTruthy();
});

test("the inbox marks the wave the request is on", async () => {
  await load("/wave/board", { ctx: open("alice") });
  const open_ = Array.from(document.querySelectorAll(".wave-card.open")).map((a) => a.getAttribute("href"));
  expect(open_, "the card for the open wave is the one marked").toEqual(["/wave/board"]);
});

test("the rail keeps the wave under every view", async () => {
  await load("/wave/kickoff", { ctx: open("alice") });
  const hrefs = Array.from(document.querySelectorAll(".rail a")).map((a) => a.getAttribute("href"));
  expect(hrefs, "each view is this page with a different query").toEqual(["/wave/kickoff?view=inbox", "/wave/kickoff?view=active", "/wave/kickoff?view=mine"]);
  expect(document.querySelectorAll(".view.on").length, "one view is current").toEqual(1);
});

test("the contacts pane shows presence across every wave", async () => {
  await load("/wave/kickoff", { ctx: open("alice") });
  const names = Array.from(document.querySelectorAll(".contact")).map((li) => li.className);
  expect(names, "alice is here and bob is not").toEqual(["contact on", "contact"]);
});

test("a blip's body is its parts rendered as elements", async () => {
  await load("/wave/kickoff", { ctx: open("alice") });
  const first = document.querySelector(`${FIRST_BLOCK} [data-sf-children] > p`);
  expect(first?.querySelector("strong")?.textContent, "the markdown arrived as elements rather than as asterisks").toEqual("a");
  expect(first?.textContent, "with its text around them").toEqual("Starting a wave.");
});

test("a window's own rewrite never takes its blip away from it", async () => {
  await load("/wave/kickoff", { ctx: open("alice") });
  const edits = key<{ blip: string; block: string; who: string; body: string }[]>("wave/edits");
  set(edits, [{ blip: "1", block: "", who: "alice", body: "mine" }]);
  await settle();
  expect(document.querySelectorAll(".blips .body-held").length, "alice's own rewrite, which a uniform view carries, is not shown to her as someone else's").toEqual(0);
  set(edits, [{ blip: "1", block: "", who: "bob", body: "his" }]);
  await settle();
  expect(document.querySelectorAll(".blips .body-held").length, "bob's rewrite still holds the blip in alice's window").toEqual(1);
});

test("someone typing beside a block shows there and nowhere else", async () => {
  await load("/wave/kickoff", { ctx: open("alice") });
  set(key<{ who: string; parent: string; anchor: string; body: string }[]>("wave/drafts"), [{ who: "bob", parent: "1", anchor: "b1", body: "beside" }]);
  await settle();
  expect(document.querySelectorAll(".blip.ghost").length, "one ghost").toEqual(1);
  expect(document.querySelectorAll(`${FIRST_BLOCK} > sf-s .blip.ghost`).length, "beside the paragraph bob is answering").toEqual(1);
});

test("someone typing with their words kept back shows who is typing and nothing of what", async () => {
  await load("/wave/kickoff", { ctx: open("alice") });
  set(key<{ who: string; parent: string; anchor: string; body: string }[]>("wave/drafts"), [{ who: "bob", parent: "", anchor: "", body: "" }]);
  await settle();
  const ghost = document.querySelector(".transcript > sf-s .blip.ghost");
  expect(ghost?.textContent, "above the wave's composer").toEqual("bobis typing");
  expect(ghost?.querySelector(".body"), "with no words").toBeNull();
});

test("the composer keeps the words back until Show what I type is ticked", async () => {
  await load("/wave/kickoff", { ctx: open("alice") });
  const shown = document.querySelector(".transcript > sf-s .composer .showing input") as HTMLInputElement;
  expect(shown.checked, "the box starts unticked").toBeFalsy();
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
  expect(asked, "the action carried the paragraph's block").toEqual([{ parent: "1", anchor: "b1", body: "a word on that" }]);
});

/// DEFECTS 5.3: the client action path and the revalidation it triggers.
test("a wave opens at the end of its transcript", async () => {
  const into = spyOn(Element.prototype, "scrollIntoView");
  await load("/wave/kickoff", { ctx: open("alice") });
  expect(into, "the end is brought into view at once").toHaveBeenCalledWith({ behavior: "auto", block: "end" });
  into.mockRestore();
});

test("keeping a blip calls the action and the transcript follows without a reload", async () => {
  const blips = [blip("1", "", "", "alice", "Starting a wave.")];
  const live = ctx({
    session: { name: "dora", waves: {} },
    services: {
      waves: {
        getWave: () => ({ id: "kickoff", title: "Snapfire kickoff", participants: ["alice"], ...clock, blips }),
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
  expect(document.querySelectorAll(".blips > .thread").length, "one blip to begin with").toEqual(1);

  const composer = document.querySelector(".transcript > sf-s .composer input") as HTMLInputElement;
  await fireEvent.change(composer, "written through the action client");
  await fireEvent.submit(composer);

  const bodies = Array.from(document.querySelectorAll(".blips .body.md > .block [data-sf-children] > p")).map((b) => b.textContent?.trim());
  expect(bodies, `the action ran and the page revalidated in place, got ${bodies.join(" | ")}`).toEqual(["Starting a wave.", "written through the action client"]);
  expect(document.querySelectorAll(".blips > .thread").length, "and the new blip is one item, in its own region").toEqual(2);
});

test("a blip the reader keeps out of view is brought into view", async () => {
  const blips = [blip("1", "", "", "alice", "Starting a wave.")];
  const live = ctx({
    session: { name: "dora", waves: {} },
    services: {
      waves: {
        getWave: () => ({ id: "kickoff", title: "Snapfire kickoff", participants: ["alice"], ...clock, blips }),
        addBlip: (input: { body: string }) => {
          const kept = blip(String(blips.length + 1), "", "", "dora", input.body);
          blips.push(kept);
          return kept;
        },
        ...listings,
      },
    },
  });
  await load("/wave/kickoff", { ctx: live });
  Object.defineProperty(window, "innerHeight", { configurable: true, value: 900 });
  const place = spyOn(Element.prototype, "getBoundingClientRect").mockImplementation(function (this: Element) {
    const top = this.id === "blip-2" ? 1200 : 0;
    const height = this.classList.contains("transcript") ? 800 : 100;
    return { x: 0, y: top, width: 400, height, top, left: 0, right: 400, bottom: top + height, toJSON: () => ({}) };
  });
  const scrolled = spyOn(Element.prototype, "scrollIntoView");

  const transcript = document.querySelector(".transcript");
  const composer = document.querySelector(".transcript > sf-s .composer input") as HTMLInputElement;
  await fireEvent.change(composer, "below the fold");
  await fireEvent.submit(composer);

  expect(document.querySelector(".transcript"), "the transcript is the element that was there, so it keeps its scroll").toBe(transcript);
  expect(scrolled.mock.contexts.map((el) => (el as Element).id), "the kept blip and nothing else").toEqual(["blip-2"]);
  scrolled.mockRestore();
  place.mockRestore();
  delete (window as { innerHeight?: number }).innerHeight;
});

test("someone rewriting a block shows their words in that block and leaves the rest of the blip alone", async () => {
  await load("/wave/kickoff", { ctx: open("alice") });
  set(key<{ blip: string; block: string; who: string; body: string }[]>("wave/edits"), [{ blip: "1", block: "b1", who: "bob", body: "his words" }]);
  await settle();
  const held = document.querySelectorAll(`${FIRST_BLOCK} .block-held`);
  expect(held.length, "the block bob holds shows his words").toEqual(1);
  expect(held[0].querySelector(".body")?.textContent).toEqual("his words");
  expect(document.querySelectorAll(".blips .body-held").length, "the blip as a whole is not held").toEqual(0);
});

test("rewriting one block sends that block to the amend action", async () => {
  const asked: { blip: string; block: string; body: string }[] = [];
  const live = ctx({
    session: { name: "dora", waves: {} },
    services: {
      waves: {
        getWave: () => wave,
        editBlip: (input: { blip: string; block: string; body: string }) => {
          asked.push({ blip: input.blip, block: input.block, body: input.body });
          return wave.blips[0];
        },
        ...listings,
      },
    },
  });
  await load("/wave/kickoff", { ctx: live });
  await fireEvent.click(document.querySelector(`${FIRST_BLOCK} > sf-s .edit-block`) as Element);
  const area = document.querySelector(`${FIRST_BLOCK} > sf-s textarea`) as HTMLTextAreaElement;
  expect(area.value, "the form starts from the block's markdown").toEqual("Starting **a** wave.");
  await fireEvent.change(area, "Starting **the** wave.");
  await fireEvent.submit(area);
  expect(asked).toEqual([{ blip: "1", block: "b1", body: "Starting **the** wave." }]);
});

test("a block whose children change keeps the island inside them as it stands", async () => {
  const later: Blip[] = [];
  const listed = (): Block => ({ id: "b1", text: "- one\n- two", parts: [list([item("b1.0", [text("one")]), item("b1.1", [text("two")], later)])], gadget: none });
  const live = ctx({
    session: { name: "dora", waves: {} },
    services: {
      waves: {
        getWave: () => ({ ...wave, blips: [blip("1", "", "", "alice", "- one\n- two", { blocks: [listed()] })] }),
        addBlip: (input: { parent: string; anchor: string; body: string }) => {
          const kept = blip("9", input.parent, input.anchor, "dora", input.body);
          later.push(kept);
          return kept;
        },
        ...listings,
      },
    },
  });
  await load("/wave/kickoff", { ctx: live });
  const items = () => document.querySelectorAll(".blips .body.md li");
  await fireEvent.click(items()[0].querySelector(".reply") as Element);
  const draft = items()[0].querySelector(".composer input") as HTMLInputElement;
  await fireEvent.change(draft, "half a reply");

  const composer = document.querySelector(".transcript > sf-s .composer input") as HTMLInputElement;
  await fireEvent.change(composer, "under the second");
  await fireEvent.submit(composer);

  expect(items()[1].querySelector(".asides")?.textContent ?? "", "the reply to the second item came back under it").toContain("under the second");
  expect(items()[0].querySelector(".composer input"), "the composer under the first item is the one that was there").toBe(draft);
  expect(draft.value, "with what was typed in it").toEqual("half a reply");
});

test("a reply composer closes with its cancel button or Escape", async () => {
  await load("/wave/kickoff", { ctx: open("alice") });
  const composers = () => document.querySelectorAll(`${FIRST_BLOCK} > sf-s .composer`).length;
  await fireEvent.click(document.querySelector(`${FIRST_BLOCK} > sf-s .reply`) as Element);
  expect(composers(), "the composer is open").toEqual(1);
  await fireEvent.click(document.querySelector(`${FIRST_BLOCK} > sf-s .composer .cancel`) as Element);
  expect(composers(), "and the cancel button closes it").toEqual(0);

  await fireEvent.click(document.querySelector(`${FIRST_BLOCK} > sf-s .reply`) as Element);
  await fireEvent.keyDown(document.querySelector(`${FIRST_BLOCK} > sf-s .composer input`) as Element, { key: "Escape" });
  expect(composers(), "so does Escape").toEqual(0);
  expect(document.querySelector(`${FIRST_BLOCK} > sf-s .reply`), "which offers the reply again").toBeTruthy();
});

/** Three changes into five, the third bob's rewrite of alice's blip. */
const replaying = {
  ...wave,
  step: 3,
  steps: 5,
  live: false,
  change: { kind: "amended", who: "bob", at: "09:20", blip: "1" },
  blips: [
    blip("1", "", "", "alice", "Starting a wave.", { lit: true, edited: "09:20", editors: ["bob"], replies: [blip("2", "1", "", "bob", "Under the first.")] }),
    blip("5", "", "", "bob", "", { blocks: [{ id: "b1", text: "```gadget noughts\n```", parts: [gadgetPart("b1")], gadget: noughts }] }),
  ],
};

test("a step of playback lights what it changed and offers nothing to write with", async () => {
  await load("/wave/kickoff?at=3", { ctx: ctx({ session: { name: "alice", waves: {} }, services: { waves: { getWave: () => replaying, ...listings } } }) });
  expect(Array.from(document.querySelectorAll(".blip.lit > .who")).map((who) => who.textContent), "the blip the step rewrote is lit").toEqual(["alice"]);
  expect(document.querySelector(".playback .count")?.textContent).toEqual("5 changes");
  expect(document.querySelector(".blips .edited")?.textContent, "the stamp says when as it did then").toEqual("edited 09:20");
  expect(document.querySelectorAll(".under").length, "no composer, no reply and no ghost anywhere").toEqual(0);
  expect(document.querySelectorAll(".take, .edit-block").length, "and no blip or block offers itself for rewriting").toEqual(0);
  expect(document.querySelectorAll(".gadget .cell[disabled]").length, "the board is as it stood and cannot be played").toEqual(9);
  expect(document.querySelector(".gadget .again"), "or cleared").toBeNull();
});

test("playback opens from the bar under the title at the last step, every move takes the place of its history entry and the exit closes it", async () => {
  const asked: string[] = [];
  const live = ctx({
    session: { name: "alice", waves: {} },
    services: {
      waves: {
        getWave: (input: { at?: string }) => {
          asked.push(input.at ?? "");
          return input.at ? { ...replaying, step: Number(input.at) } : wave;
        },
        ...listings,
      },
    },
  });
  await load("/wave/kickoff", { ctx: live });
  expect(document.querySelector(".playback"), "no scrubber on the wave as it stands").toBeNull();
  const entries = history.length;
  const top = spyOn(window, "scrollTo");
  const into = spyOn(Element.prototype, "scrollIntoView");

  await fireEvent.click(document.querySelector(".wave-nav .open-playback") as Element);
  expect(location.pathname + location.search, "where the wave stands").toEqual("/wave/kickoff?at=5");
  expect(into, "with the end of the transcript brought into view").toHaveBeenCalledWith({ behavior: "smooth", block: "end" });
  into.mockRestore();
  const bar = document.querySelector(".playback");
  expect(bar, "the bar opened").toBeTruthy();
  expect(document.querySelector(".wave-nav .open-playback"), "in place of the button").toBeNull();
  expect(document.querySelector(".playback .count")?.textContent).toEqual("5 changes");

  await fireEvent.click(document.querySelector('.playback [title="a step back"]') as Element);
  expect(location.pathname + location.search).toEqual("/wave/kickoff?at=4");
  expect(top, "the window stays where the reader left it").not.toHaveBeenCalled();
  top.mockRestore();
  expect(asked, "the loader asked the service for each step").toEqual(["", "5", "4"]);
  expect(history.length, "every move took the place of the entry it moved from").toEqual(entries);
  expect(document.querySelector(".playback"), "the scrubber is the island that was there").toBe(bar);
  expect(document.querySelectorAll(".composer").length, "and playback offers nothing to write with").toEqual(0);

  await fireEvent.click(document.querySelector(".playback .exit") as Element);
  expect(location.pathname + location.search, "the exit is the wave as it stands").toEqual("/wave/kickoff");
  expect(document.querySelector(".playback"), "with the bar closed").toBeNull();
  expect(document.querySelector(".wave-nav .open-playback"), "and the button back").toBeTruthy();
});

test("a gadget sits in its blip where its fence is and an answer goes to the action with its blip and block", async () => {
  const asked: { blip: string; block: string; answer: string }[] = [];
  const pick = blip("1", "", "", "alice", "Pick one.", {
    blocks: [
      { id: "b1", text: "Pick one.", parts: [para("b1", [text("Pick one.")])], gadget: none },
      { id: "b2", text: "```gadget poll\nWhen?\nThursday\nFriday\n```", parts: [gadgetPart("b2")], gadget: poll },
    ],
  });
  const live = ctx({
    session: { name: "alice", waves: {} },
    services: {
      waves: {
        getWave: () => ({ ...wave, blips: [pick] }),
        vote: (input: { blip: string; block: string; answer: string }) => {
          asked.push({ blip: input.blip, block: input.block, answer: input.answer });
          return pick;
        },
        ...listings,
      },
    },
  });
  await load("/wave/kickoff", { ctx: live });
  const gadget = document.querySelector(".blips .body.md > .block > sf-s[data-sf-mode=server] .gadget") as Element;
  expect(gadget.querySelector(".question")?.textContent, "the blip's second block is its poll").toEqual("When?");
  expect(Array.from(gadget.querySelectorAll(".choice")).map((choice) => choice.textContent), "each answer with how many gave it and who").toEqual(["Thursday1bob", "Friday0"]);
  await fireEvent.click(gadget.querySelectorAll(".choice button")[1]);
  expect(asked, "the action carried the gadget's blip and block").toEqual([{ blip: "1", block: "b2", answer: "Friday" }]);
});

test("a poll's author closes it for good and a closed poll or one past its deadline takes no answer", async () => {
  const closes: { id: string; blip: string; block: string; who: string }[] = [];
  const polled = (gadget: Gadget) => blip("1", "", "", "alice", "", { blocks: [{ id: "b2", text: "```gadget poll\nWhen?\nThursday\nFriday\n```", parts: [gadgetPart("b2")], gadget }] });
  const reading = (name: string, gadget: Gadget) =>
    ctx({
      session: { name, waves: {} },
      services: {
        waves: {
          getWave: () => ({ ...wave, blips: [polled(gadget)] }),
          closeVote: (input: { id: string; blip: string; block: string; who: string }) => {
            closes.push(input);
            return polled({ ...gadget, closed: true });
          },
          ...listings,
        },
      },
    });
  const shown = () => document.querySelector(".gadget.votes .closing")?.textContent ?? "";
  const answerable = () => Array.from(document.querySelectorAll(".gadget.votes .choice button")).some((button) => !(button as HTMLButtonElement).disabled);

  await load("/wave/kickoff", { ctx: reading("bob", { ...poll, ends: "15 Sep 18:00 UTC" }) });
  expect(shown(), "a poll says when voting ends").toEqual("Ends 15 Sep 18:00 UTC");
  expect(document.querySelector(".gadget.votes .close"), "and only its author is offered to close it").toBeNull();

  await load("/wave/kickoff", { ctx: reading("alice", poll) });
  await fireEvent.click(document.querySelector(".gadget.votes .close") as Element);
  expect(closes, "closing goes to the service as its author").toEqual([{ id: "kickoff", blip: "1", block: "b2", who: "alice" }]);

  await load("/wave/kickoff", { ctx: reading("alice", { ...poll, closed: true }) });
  expect(shown()).toEqual("Closed");
  expect(document.querySelector(".gadget.votes .close"), "there is no reopening").toBeNull();
  expect(answerable(), "a closed poll takes no answer").toBeFalsy();

  await load("/wave/kickoff", { ctx: reading("bob", { ...poll, ended: true, ends: "15 Sep 18:00 UTC" }) });
  expect(shown()).toEqual("Ended 15 Sep 18:00 UTC");
  expect(answerable(), "and neither does one past its deadline").toBeFalsy();
});

test("a gadget keeps its props as the server encoded them when a revalidation hands it new ones", async () => {
  const counts = { thursday: 1, friday: 0 };
  const polled = () =>
    blip("1", "", "", "alice", "Pick one.", {
      blocks: [
        {
          id: "b1",
          text: "```gadget poll\nWhen?\nThursday\nFriday\n```",
          parts: [gadgetPart("b1")],
          gadget: { ...poll, choices: [{ answer: "Thursday", count: counts.thursday, who: ["bob"] }, { answer: "Friday", count: counts.friday, who: counts.friday > 0 ? ["alice"] : [] }] },
        },
      ],
    });
  const live = ctx({
    session: { name: "alice", waves: {} },
    services: {
      waves: {
        getWave: () => ({ ...wave, blips: [polled()] }),
        vote: () => {
          counts.friday += 1;
          return polled();
        },
        ...listings,
      },
    },
  });
  await load("/wave/kickoff", { ctx: live });
  const warned = spyOn(console, "warn");
  await fireEvent.click(document.querySelector(".gadget.votes .choice:nth-child(2) button") as Element);
  await settle();
  expect(warned, "no step of the gadget failed").not.toHaveBeenCalled();
  expect(document.querySelector(".gadget.votes .choice:nth-child(2)")?.textContent, "the page revalidated and the gadget took the new count").toEqual("Friday1alice");
  const island = document.querySelector(".gadget.votes")?.closest("sf-i");
  const script = document.querySelector(`script[data-sf-props="${island?.id}"]`);
  expect(script?.textContent ?? "", "a count is still written as the float it is").toContain('"$":"f"');
  warned.mockRestore();
});

async function composing(): Promise<{ bodies: string[]; composer: HTMLFormElement; pick: (name: string) => Element }> {
  const bodies: string[] = [];
  const live = ctx({
    session: { name: "alice", waves: {} },
    services: {
      waves: {
        getWave: () => wave,
        addBlip: (input: { body: string }) => {
          bodies.push(input.body);
          return blip("9", "", "", "alice", input.body);
        },
        ...listings,
      },
    },
  });
  await load("/wave/kickoff", { ctx: live });
  const composer = document.querySelector(".transcript > sf-s .composer") as HTMLFormElement;
  const pick = (name: string) => Array.from(composer.querySelectorAll(".add-gadget button")).find((button) => button.textContent === name) as Element;
  return { bodies, composer, pick };
}

test("the composer's gadget menu opens a vote in an editor shaped like it and keeps it as a poll", async () => {
  const { bodies, composer, pick } = await composing();
  await fireEvent.change(composer.querySelector("input") as HTMLInputElement, "Ship on Friday?");
  await fireEvent.click(pick("Yes / No / Maybe"));
  expect(bodies, "nothing is kept until the editor sends it").toEqual([]);
  expect(composer.querySelector(".gadget.votes .choices"), "the editor is the vote's own markup").toBeTruthy();
  expect((composer.querySelector(".gadget-editor .question") as HTMLInputElement).value, "what was typed is the question").toEqual("Ship on Friday?");
  const answers = () => Array.from(composer.querySelectorAll(".gadget-editor .answer")).map((input) => (input as HTMLInputElement).value);
  expect(answers(), "a yes/no starts with its three answers, each one editable").toEqual(["Yes", "No", "Maybe"]);
  await fireEvent.click(composer.querySelectorAll(".gadget-editor .drop")[2] as Element);
  expect(answers(), "and removable").toEqual(["Yes", "No"]);
  expect(composer.querySelector('button[type="submit"]')?.textContent).toEqual("Add to wave");
  await fireEvent.submit(composer);
  expect(bodies).toEqual(["```gadget poll\nShip on Friday?\nYes\nNo\n```"]);
  expect(composer.querySelector(".gadget-editor"), "and the editor closes").toBeNull();
});

test("a poll is written in the editor with its answers, two of them at least", async () => {
  const { bodies, composer, pick } = await composing();
  const into = spyOn(Element.prototype, "scrollIntoView");
  await fireEvent.click(pick("Poll"));
  expect(into, "the editor is brought into view as it opens").toHaveBeenCalledWith({ behavior: "smooth", block: "nearest" });
  into.mockRestore();
  const submit = composer.querySelector('button[type="submit"]') as HTMLButtonElement;
  expect(submit.disabled, "a poll with no answers written cannot be kept").toBeTruthy();
  await fireEvent.change(composer.querySelector(".gadget-editor .question") as HTMLInputElement, "When?");
  const choice = (i: number) => composer.querySelectorAll(".gadget-editor .answer")[i] as HTMLInputElement;
  await fireEvent.change(choice(0), "Thursday");
  await fireEvent.change(choice(1), "Friday");
  await fireEvent.click(composer.querySelector(".gadget-editor .more") as Element);
  await fireEvent.change(choice(2), "Saturday");
  await fireEvent.click(composer.querySelectorAll(".gadget-editor .drop")[1] as Element);
  const written = Array.from(composer.querySelectorAll(".gadget-editor .answer")).map((input) => (input as HTMLInputElement).value);
  expect(written, "an answer can be added and one removed").toEqual(["Thursday", "Saturday"]);
  expect(submit.disabled).toBeFalsy();
  await fireEvent.change(composer.querySelector(".gadget-editor .until input") as HTMLInputElement, "2026-09-15T18:00");
  await fireEvent.submit(composer);
  const until = `${new Date("2026-09-15T18:00").toISOString().slice(0, 16)}Z`;
  expect(bodies, "the deadline goes on the fence as a UTC minute").toEqual(["```gadget poll until=" + until + "\nWhen?\nThursday\nSaturday\n```"]);
});

test("a board from the gadget menu is kept at once, with what was typed above it", async () => {
  const { bodies, composer, pick } = await composing();
  await fireEvent.change(composer.querySelector("input") as HTMLInputElement, "Your move");
  await fireEvent.click(pick("Noughts and crosses"));
  expect(bodies).toEqual(["Your move\n\n```gadget noughts\n```"]);
});

test("a reader with no name is asked for one across the top and the side pane names nobody", async () => {
  await load("/wave/kickoff", { ctx: open("") });
  expect(document.querySelectorAll(".naming .name input").length, "the bar with the name form").toEqual(1);
  expect(document.querySelector(".side-foot .wordmark")?.textContent).toEqual("Waves");
  expect(document.querySelector(".side-foot .me"), "and no name at its foot").toBeNull();
});

test("a named reader's name opens their settings, where it can be changed", async () => {
  await load("/wave/kickoff", { ctx: open("alice") });
  expect(document.querySelector(".naming"), "no bar once there is a name").toBeNull();
  const me = document.querySelector(".side-foot .me") as Element;
  expect(me.textContent).toEqual("alice");
  await fireEvent.click(me);
  const input = document.querySelector(".modal .sheet .name input") as HTMLInputElement;
  expect(input.value, "the settings hold the name as it is").toEqual("alice");
  await fireEvent.keyDown(input, { key: "Escape" });
  expect(document.querySelector(".modal"), "and Escape closes them").toBeNull();
});

test("opening a reply closes the one already open", async () => {
  await load("/wave/kickoff", { ctx: open("alice") });
  await fireEvent.click(document.querySelector(".blips > .thread > sf-s .reply") as Element);
  await fireEvent.click(document.querySelector(`${FIRST_BLOCK} > sf-s .reply`) as Element);
  expect(document.querySelectorAll(".blips .composer").length, "one reply composer at a time").toEqual(1);
  expect(document.querySelector(`${FIRST_BLOCK} > sf-s .composer`), "the one opened last").toBeTruthy();
  expect(document.querySelectorAll(".composer").length, "beside the wave's own, which stays open").toEqual(2);
});
