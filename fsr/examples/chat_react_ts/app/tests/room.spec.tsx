import { ctx, expect, fireEvent, load, test, waitFor } from "@snapfire/fsr-client/testing";

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

test("the room shows the figures the loader asked ctx.native for", async () => {
  await load("/room/lobby", { ctx: room("alice") });
  expect(document.querySelector(".digest .words")?.textContent).toEqual("4 words");
  expect(document.querySelector(".digest .longest")?.textContent).toEqual("Coffee is on.");
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

test("a reader with no name is sent to name themselves and comes back to the room", async () => {
  await load("/room/lobby", { ctx: room("") });
  expect(document.querySelector(".say"), "no composer before a name").toBeNull();
  const link = document.querySelector(".unnamed a");
  expect(link?.getAttribute("href")).toEqual("/?next=/room/lobby");

  await load("/?next=/room/lobby", { ctx: room("") });
  const field = document.querySelector<HTMLInputElement>(".name input");
  field!.value = "carol";
  await fireEvent.submit(document.querySelector(".name")!);
  await waitFor(() => expect(location.pathname).toEqual("/room/lobby"));
  expect(document.querySelector(".say input"), "named, the composer is there").toBeTruthy();
});
