import { assert, ctx, load, test } from "@snapfire/fsr-client/testing";

const transcript = {
  room: { id: "lobby", name: "Lobby", about: "Anyone, anything", messages: 2 },
  messages: [
    { id: "1", who: "alice", body: "Morning.", at: "09:01" },
    { id: "2", who: "bob", body: "Coffee is on.", at: "09:02" },
  ],
};

const room = (name: string) =>
  ctx({
    session: { name, rooms: {} },
    services: { rooms: { getRoom: () => transcript, listRooms: () => [transcript.room] } },
  });

test("a room renders its transcript and marks what this reader said", async () => {
  await load("/room/lobby", { ctx: room("bob") });
  const said = Array.from(document.querySelectorAll(".said"));
  assert.equal(said.length, 2, "one line per message");
  assert.equal(said[0]?.className, "said", "alice's line belongs to alice");
  assert.equal(said[1]?.className, "said mine", "bob's own line is marked");
  assert.equal(document.querySelector(".said .body")?.textContent, "Morning.");
});

test("the room carries the island that follows it", async () => {
  await load("/room/lobby", { ctx: room("alice") });
  assert.ok(document.querySelector(".live"), "the live pill is on the room");
  assert.ok(document.querySelector(".say input"), "and so is the composer");
});

test("the room list links into each room", async () => {
  await load("/", { ctx: room("alice") });
  const hrefs = Array.from(document.querySelectorAll("a")).map((a) => a.getAttribute("href"));
  assert.ok(hrefs.includes("/room/lobby"), `a link per room, got ${hrefs.join(" ")}`);
});
