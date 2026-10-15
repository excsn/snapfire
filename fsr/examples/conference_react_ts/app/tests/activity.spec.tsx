import { ctx, expect, fireEvent, load, screen, settle, test } from "@snapfire/fsr-client/testing";

const talks = [
  { id: "1", title: "A plan is not a program", track: "Runtime", room: "Hall B", starts: "09:30", ends: "10:10", speaker: "Ada Okonjo", level: "intro", abstract: "" },
  { id: "2", title: "Loaders that refuse to waterfall", track: "Data", room: "Room 2", starts: "09:30", ends: "10:10", speaker: "Bea Lindqvist", level: "intermediate", abstract: "" },
];

const conference = { name: "Everything At Once", day: "Thursday 18 September", venue: "The Old Exchange", tracks: ["Runtime", "Data"] };

const day = () =>
  ctx({
    services: {
      program: { getConference: () => conference, listTalks: () => talks, listAnnouncements: () => [], listSponsors: () => [] },
    },
  });

async function leaveTheVenue() {
  await load("/venue", { ctx: day() });
  await settle();
  const note = document.querySelector("textarea[name=note]") as HTMLTextAreaElement;
  note.value = "find the coffee";
  const count = document.querySelector("talk-count") as HTMLElement & { seen?: number };
  count.seen = 1;
  await fireEvent.click(screen.getByText("Room 2"));
  await settle();
  expect(document.querySelector(".floors-chosen")?.textContent, "the Vue island holds a room").toEqual("Room 2");
  await fireEvent.click(screen.getByText("Schedule"));
  await settle();
  expect(location.pathname).toEqual("/");
  expect(document.querySelector("textarea[name=note]"), "the venue left the document").toBeNull();
  return { note, count };
}

test("a step back puts the venue back as the visitor left it, with the framework islands mounted afresh", async () => {
  const { note, count } = await leaveTheVenue();
  history.back();
  await settle();
  expect(location.pathname).toEqual("/venue");
  expect(document.querySelector("textarea[name=note]"), "the same textarea came back").toBe(note);
  expect(note.value, "with what was typed in it").toEqual("find the coffee");
  expect(document.querySelector("talk-count"), "the custom element came back as the same element").toBe(count);
  expect(count.seen, "with what was set on it").toEqual(1);
  expect(document.querySelector(".floors-room"), "the Vue island mounted again").toBeTruthy();
  expect(document.querySelector(".floors-chosen"), "from the payload, so its room is not chosen").toBeNull();
  expect(document.querySelector('sf-i[data-sf-module="src/ui/LocalClock.tsx#default"][data-sf-mounted] .local-clock'), "the React island mounted again").toBeTruthy();
});

test("a link back to the venue puts it back too", async () => {
  const { note } = await leaveTheVenue();
  await fireEvent.click(screen.getByText("Venue"));
  await settle();
  expect(location.pathname).toEqual("/venue");
  expect(document.querySelector("textarea[name=note]"), "the held page came back").toBe(note);
  expect(note.value).toEqual("find the coffee");
});

test("a link that says data-sf-activity none renders the page afresh", async () => {
  const { note } = await leaveTheVenue();
  const venue = screen.getByText("Venue");
  venue.setAttribute("data-sf-activity", "none");
  await fireEvent.click(venue);
  await settle();
  expect(location.pathname).toEqual("/venue");
  const fresh = document.querySelector("textarea[name=note]") as HTMLTextAreaElement;
  expect(fresh !== note && fresh.value === "", "a new textarea, empty").toBeTruthy();
});
