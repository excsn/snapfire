import { assert, ctx, load, test } from "@snapfire/fsr-client/testing";

const wave = {
  id: "kickoff",
  title: "Snapfire kickoff",
  participants: ["alice", "bob"],
  blips: [
    { id: "1", parent: "", who: "alice", body: "Starting a wave.", at: "09:10", depth: 0 },
    { id: "2", parent: "1", who: "bob", body: "Under the first.", at: "09:12", depth: 1 },
  ],
};

const open = (name: string) =>
  ctx({
    session: { name, waves: {} },
    services: {
      waves: {
        getWave: () => wave,
        listWaves: () => [{ id: "kickoff", title: "Snapfire kickoff", participants: ["alice", "bob"], blips: 2, last: "09:12" }],
      },
    },
  });

test("the wave renders every blip at the depth the service gave it", async () => {
  await load("/wave/kickoff", { ctx: open("bob") });
  const items = Array.from(document.querySelectorAll(".blips li"));
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

test("the inbox lists every wave beside the open one", async () => {
  await load("/wave/kickoff", { ctx: open("alice") });
  const hrefs = Array.from(document.querySelectorAll(".wave-list a")).map((a) => a.getAttribute("href"));
  assert.ok(hrefs.includes("/wave/kickoff"), `the list is a slot of the layout, got ${hrefs.join(" ")}`);
});
