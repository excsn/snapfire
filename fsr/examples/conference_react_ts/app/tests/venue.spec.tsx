import { ctx, expect, fireEvent, load, render, screen, settle, test } from "@snapfire/fsr-client/testing";

import LocalClock from "@src/ui/LocalClock";

const talks = [
  { id: "1", title: "A plan is not a program", track: "Runtime", room: "Hall B", starts: "09:30", ends: "10:10", speaker: "Ada Okonjo", level: "intro", abstract: "" },
  { id: "2", title: "Loaders that refuse to waterfall", track: "Data", room: "Room 2", starts: "09:30", ends: "10:10", speaker: "Bea Lindqvist", level: "intermediate", abstract: "" },
  { id: "3", title: "Caching what you meant", track: "Data", room: "Room 2", starts: "10:20", ends: "11:00", speaker: "Cy Mbeki", level: "intro", abstract: "" },
];

const conference = { name: "Everything At Once", day: "Thursday 18 September", venue: "The Old Exchange", tracks: ["Runtime", "Data"] };

const day = () =>
  ctx({
    services: {
      program: { getConference: () => conference, listTalks: () => talks, listAnnouncements: () => [], listSponsors: () => [] },
    },
  });

test("the venue page is composition with an island the browser renders alone and a Vue island in it", async () => {
  await load("/venue", { ctx: day() });
  await settle();
  expect(screen.getByText("The Old Exchange"), "the page is the server's markup").toBeTruthy();
  expect(document.querySelector('sf-i[data-sf-module="routes/venue/page.tsx#default"]'), "no framework holds the page").toBeNull();
  const clock = document.querySelector('sf-i[data-sf-module="src/ui/LocalClock.tsx#default"][data-sf-mounted]');
  expect(clock, "the clock reads the visitor's time zone, which the server cannot, so the browser renders it alone").toBeTruthy();
  expect(clock!.querySelector(".local-clock")?.textContent?.startsWith("Times are the venue's. You are in "), clock!.innerHTML).toBeTruthy();
  expect(screen.getByLabelText("my schedule"), "and the layout around it").toBeTruthy();
  const rooms = document.querySelectorAll(".floors-room");
  expect(rooms.length, "one button per room").toEqual(2);
  await fireEvent.click(screen.getByText("Room 2"));
  await settle();
  expect(document.querySelectorAll(".floors-talks li").length, "the island's own state").toEqual(2);
});

test("one page under one layout holds an island of every owner", async () => {
  await load("/venue", { ctx: day() });
  await settle();
  const owner = (module: string) => document.querySelector(`sf-i[data-sf-module="${module}"]`);
  expect(owner("src/ui/Saved.tsx#default")?.hasAttribute("data-sf-mounted"), "React, in the layout").toEqual(true);
  expect(owner("src/ui/Floors.vue#default")?.hasAttribute("data-sf-mounted"), "Vue").toEqual(true);
  expect(owner("src/ui/LocalClock.tsx#default")?.hasAttribute("data-sf-mounted"), "React, mounted fresh since the server cannot render it").toEqual(true);
  const count = document.querySelector("talk-count");
  expect(count?.hasAttribute("data-upgraded"), "the custom element defined itself").toEqual(true);
  expect(count?.textContent).toEqual("3 talks today");
  expect(owner("src/ui/AskDesk.tsx#default")?.closest("sf-s")?.getAttribute("data-sf-mode"), "the help desk steps on the server").toEqual("server");
  await fireEvent.click(screen.getByText("Ask the help desk"));
  await settle();
  expect(screen.getByText("Asked 1 time"), "the server answered the click").toBeTruthy();
});

test("the clock renders on its own through React, with nothing from the server to hydrate", async () => {
  const r = await render(<LocalClock />);
  expect(r.hydrated, "the server could not render it").toBeNull();
  expect(r.container.querySelector("sf-i")?.hasAttribute("data-sf-mounted"), "React's mounter ran").toEqual(true);
  expect(r.getByText(/You are in/).textContent).toMatch(/Times are the venue's\. You are in .+\./);
});

test("the walk from the station is a video whose sources, captions and poster the page imports", async () => {
  await load("/venue", { ctx: day() });
  await settle();
  const video = document.querySelector<HTMLVideoElement>("video.venue-walk")!;
  expect(video.hasAttribute("playsinline") && video.hasAttribute("controls"), video.outerHTML).toEqual(true);
  expect(video.getAttribute("poster")!.match(/^\/static\/js\/app\/src\/media\/walk-poster\.[0-9a-f]{8}\.png$/), "the poster is the image's hashed original").toBeTruthy();
  const sources = Array.from(video.querySelectorAll("source"), (s) => `${s.getAttribute("type")} ${s.getAttribute("src")}`);
  expect(sources.length).toEqual(2);
  expect(sources[0].match(/^video\/webm \/static\/js\/app\/src\/media\/walk\.[0-9a-f]{8}\.webm$/) && sources[1].match(/^video\/mp4 \/static\/js\/app\/src\/media\/walk\.[0-9a-f]{8}\.mp4$/), sources.join("\n")).toBeTruthy();
  const track = video.querySelector("track")!;
  expect(track.getAttribute("src")!.endsWith(".vtt") && track.hasAttribute("default"), track.outerHTML).toEqual(true);
});

test("the videos between the talks are stills until played, and a click puts that one player in its place", async () => {
  await load("/venue", { ctx: day() });
  await settle();
  const embeds = Array.from(document.querySelectorAll(".venue-breaks [data-sf-embed]"));
  expect(embeds.length).toEqual(2);
  expect(embeds[0].querySelector("img")!.getAttribute("src"), "the provider's still").toEqual("https://i.ytimg.com/vi/Aq5WXmQQooo/hqdefault.jpg");
  expect(document.querySelectorAll(".venue-breaks iframe").length, "no player before a click").toEqual(0);
  await fireEvent.click(embeds[1].querySelector("button")!);
  const frames = Array.from(document.querySelectorAll(".venue-breaks iframe"));
  expect(frames.length, "only the clicked one plays").toEqual(1);
  expect(frames[0].getAttribute("src")).toEqual("https://www.youtube-nocookie.com/embed/astISOttCQ0?autoplay=1");
  expect(frames[0].getAttribute("title")).toEqual("Gummibär - The Gummy Bear Song");
});
