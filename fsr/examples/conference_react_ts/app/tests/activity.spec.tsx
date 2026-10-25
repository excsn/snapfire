import { navigate } from "@snapfire/fsr/client";
import { ctx, expect, fireEvent, load, screen, settle, test } from "@snapfire/fsr/client/testing";

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
  await fireEvent.click(screen.getByLabelText("Coffee"));
  await settle();
  expect(document.querySelector(".checklist-count")?.textContent, "the React island holds a ticked item").toEqual("1 of 3 packed");
  const checklist = document.querySelector(".checklist");
  expect(document.querySelector(".floors-chosen")?.textContent, "the Vue island holds a room").toEqual("Room 2");
  await fireEvent.click(screen.getByText("Schedule"));
  await settle();
  expect(location.pathname).toEqual("/");
  expect(document.querySelector("textarea[name=note]"), "the venue left the document").toBeNull();
  return { note, count, checklist };
}

test("a step back puts the venue back as the visitor left it, the React island paused and the Vue island mounted afresh", async () => {
  const { note, count, checklist } = await leaveTheVenue();
  history.back();
  await settle();
  expect(location.pathname).toEqual("/venue");
  expect(document.querySelector("textarea[name=note]"), "the same textarea came back").toBe(note);
  expect(note.value, "with what was typed in it").toEqual("find the coffee");
  expect(document.querySelector("talk-count"), "the custom element came back as the same element").toBe(count);
  expect(count.seen, "with what was set on it").toEqual(1);
  expect(document.querySelector(".floors-room"), "the Vue island mounted again").toBeTruthy();
  expect(document.querySelector(".floors-chosen"), "from the payload, so its room is not chosen").toBeNull();
  expect(document.querySelector(".checklist"), "React 19.2 and later pause a root under <Activity>, so the same checklist came back").toBe(checklist);
  expect(document.querySelector(".checklist-count")?.textContent, "with its state").toEqual("1 of 3 packed");
  expect((screen.getByLabelText("Coffee") as HTMLInputElement).checked).toEqual(true);
  expect(document.querySelector('sf-i[data-sf-module="src/ui/LocalClock.tsx#default"][data-sf-mounted] .local-clock'), "the clock is there too").toBeTruthy();
});

test("a step back returns the window to where the visitor left the venue", async () => {
  await load("/venue", { ctx: day() });
  await settle();
  window.scrollTo(0, 300);
  // A browser moves the scroll as soon as the document changes under it: scroll anchoring on an insertion, a clamp on a removal.
  const patched: [object, string, (...args: never[]) => unknown][] = [];
  for (let proto: object | null = Object.getPrototypeOf(document.body); proto; proto = Object.getPrototypeOf(proto)) {
    for (const name of ["removeChild", "insertBefore", "appendChild", "replaceChild"]) {
      if (!Object.prototype.hasOwnProperty.call(proto, name)) continue;
      const original = (proto as Record<string, (...args: never[]) => unknown>)[name];
      patched.push([proto, name, original]);
      (proto as Record<string, unknown>)[name] = function (this: Node, ...args: never[]) {
        const result = original.apply(this, args);
        window.scrollTo(0, window.scrollY + 135);
        return result;
      };
    }
  }
  try {
    await fireEvent.click(screen.getByText("Schedule"));
    await settle();
  } finally {
    for (const [proto, name, original] of patched) (proto as Record<string, unknown>)[name] = original;
  }
  expect(location.pathname).toEqual("/");
  window.scrollTo(0, 0);
  history.back();
  await settle();
  expect(location.pathname).toEqual("/venue");
  expect(window.scrollY).toEqual(300);
});

test("a step forward returns the window to where the visitor left the schedule", async () => {
  await load("/venue", { ctx: day() });
  await settle();
  window.scrollTo(0, 700);
  await fireEvent.click(screen.getByText("Schedule"));
  await settle();
  window.scrollTo(0, 300);
  // A browser restores the entry it steps to on its own, between popstate and the navigator's patch.
  const restore = () => window.scrollTo(0, 700);
  window.addEventListener("popstate", restore);
  history.back();
  await settle();
  window.removeEventListener("popstate", restore);
  expect(location.pathname).toEqual("/venue");
  history.forward();
  await settle();
  expect(location.pathname).toEqual("/");
  expect(window.scrollY).toEqual(300);
});

test("a step back to a page activity did not hold returns the window to where it was", async () => {
  await load("/venue", { ctx: day() });
  await settle();
  window.scrollTo(0, 400);
  await navigate("/", true, { activity: false });
  await settle();
  expect(document.querySelector("textarea[name=note]"), "the venue was rendered afresh rather than held").toBeNull();
  window.scrollTo(0, 0);
  history.back();
  await settle();
  expect(location.pathname).toEqual("/venue");
  expect(window.scrollY).toEqual(400);
});

test("a step back scrolls once the paused island is shown again", async () => {
  await leaveTheVenue();
  const shown: boolean[] = [];
  const scrollTo = window.scrollTo;
  window.scrollTo = ((x: number, y: number) => {
    const checklist = document.querySelector(".checklist") as HTMLElement | null;
    shown.push(checklist !== null && checklist.style.display !== "none");
    scrollTo(x, y);
  }) as typeof window.scrollTo;
  try {
    history.back();
    await settle();
  } finally {
    window.scrollTo = scrollTo;
  }
  expect(location.pathname).toEqual("/venue");
  expect(shown.length > 0, "the step back restored a scroll").toEqual(true);
  expect(shown[shown.length - 1], "the island was shown when the window was scrolled").toEqual(true);
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
