import { ctx, expect, load, test } from "@snapfire/fsr-client/testing";

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
    native: {
      digest: {
        words: ({ bodies }: { bodies: string[] }) => bodies.reduce((n, b) => n + b.split(/\s+/).filter(Boolean).length, 0),
        longest: ({ bodies }: { bodies: string[] }) => bodies.reduce((held, b) => (b.length > held.length ? b : held), ""),
      },
    },
  });

test("a room renders its transcript and marks what this reader said", async () => {
  await load("/room/lobby", { ctx: room("bob") });
  const said = Array.from(document.querySelectorAll(".said"));
  expect(said.length, "one line per message").toEqual(2);
  expect(said[0]?.className, "alice's line belongs to alice").toEqual("said");
  expect(said[1]?.className, "bob's own line is marked").toEqual("said mine");
  expect(document.querySelector(".said .body")?.textContent).toEqual("Morning.");
});

test("the room carries the island that follows it", async () => {
  await load("/room/lobby", { ctx: room("alice") });
  expect(document.querySelector(".live"), "the live pill is on the room").toBeTruthy();
  expect(document.querySelector(".say input"), "and so is the composer").toBeTruthy();
});

test("the room list links into each room", async () => {
  await load("/", { ctx: room("alice") });
  const hrefs = Array.from(document.querySelectorAll("a")).map((a) => a.getAttribute("href"));
  expect(hrefs.includes("/room/lobby"), `a link per room, got ${hrefs.join(" ")}`).toBeTruthy();
});
