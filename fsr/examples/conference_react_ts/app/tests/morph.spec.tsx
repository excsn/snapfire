import { navigate, refresh } from "@snapfire/fsr/client";
import { ctx, expect, fireEvent, load, screen, settle, test } from "@snapfire/fsr/client/testing";

type Talk = { id: string; title: string; track: string; room: string; starts: string; ends: string; speaker: string; level: string; abstract: string };

const conference = { name: "Everything At Once", day: "Thursday 18 September", venue: "The Old Exchange", tracks: ["Runtime", "Data"] };

const programme = (): Talk[] => [
  { id: "1", title: "A plan is not a program", track: "Runtime", room: "Hall B", starts: "09:30", ends: "10:10", speaker: "Ada Okonjo", level: "intro", abstract: "" },
  { id: "2", title: "Loaders that refuse to waterfall", track: "Data", room: "Room 2", starts: "09:30", ends: "10:10", speaker: "Bea Lindqvist", level: "intermediate", abstract: "" },
];

const day = (talks: Talk[]) =>
  ctx({
    services: {
      program: { getConference: () => conference, listTalks: () => talks, listAnnouncements: () => [], listSponsors: () => [] },
    },
  });

const RENAMED = "A plan is still not a program";

function markHeading(): HTMLElement {
  const heading = document.querySelector(".schedule h2") as HTMLElement;
  heading.setAttribute("data-seen", "1");
  heading.classList.add("seen");
  return heading;
}

function expectKept(heading: HTMLElement): void {
  expect(document.querySelector(".schedule h2"), "the same heading").toBe(heading);
  expect(heading.getAttribute("data-seen"), "with the attribute a script set").toEqual("1");
  expect(heading.className, "and the class").toEqual("seen");
  expect(screen.getByText(RENAMED), "while the server's change is applied").toBeTruthy();
}

test("a refresh applies what the data changed and keeps what a script set on the document the server wrote", async () => {
  const talks = programme();
  await load("/", { ctx: day(talks) });
  await settle();
  const heading = markHeading();
  talks[0] = { ...talks[0], title: RENAMED };
  await refresh();
  await settle();
  expectKept(heading);
});

test("a refresh applies what the data changed and keeps what a script set on a page a navigation wrote", async () => {
  const talks = programme();
  await load("/venue", { ctx: day(talks) });
  await settle();
  await navigate("/", true, { activity: false });
  await settle();
  const heading = markHeading();
  talks[0] = { ...talks[0], title: RENAMED };
  await refresh();
  await settle();
  expectKept(heading);
});

test("a held page whose data changed while it was away keeps what a script set on it", async () => {
  const talks = programme();
  await load("/", { ctx: day(talks) });
  await settle();
  const heading = markHeading();
  await fireEvent.click(screen.getByText("Venue"));
  await settle();
  talks[0] = { ...talks[0], title: RENAMED };
  history.back();
  await settle();
  expectKept(heading);
});

test("a server island's step keeps what a script set on its markup", async () => {
  await load("/venue", { ctx: day(programme()) });
  await settle();
  const desk = document.querySelector(".ask-desk") as HTMLElement;
  desk.setAttribute("data-seen", "1");
  await fireEvent.click(screen.getByText("Ask the help desk"));
  await settle();
  expect(document.querySelector(".asked")?.textContent, "the step was answered").toEqual("Asked 1 time");
  expect(document.querySelector(".ask-desk"), "the same paragraph").toBe(desk);
  expect(desk.getAttribute("data-seen"), "with the attribute a script set").toEqual("1");
});
