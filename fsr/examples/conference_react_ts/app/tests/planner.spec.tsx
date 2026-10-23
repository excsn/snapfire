import { advance, ctx, expect, fireEvent, load, settle, spyOn, test } from "@snapfire/fsr-client/testing";

const day = () =>
  ctx({
    services: { program: { getConference: () => ({ name: "E", day: "d", venue: "v", tracks: [] }), listTalks: () => [], listAnnouncements: () => [], listSponsors: () => [] } },
  });

test("islands among an island's children and in its own markup read the context it provides and follow it", async () => {
  const reported = spyOn(console, "error");
  await load("/planner", { ctx: day() });
  await settle();
  expect(document.querySelector(".density-body")?.textContent, "the server rendered it in the shell's frame").toEqual("comfortable");
  expect(document.querySelector(".density-head")?.textContent).toEqual("comfortable");
  expect(reported, "hydration found what the server wrote").not.toHaveBeenCalled();
  await fireEvent.click(document.querySelector(".planner-compact") as Element);
  await settle();
  expect(document.querySelector(".density-body")?.textContent).toEqual("compact");
  expect(document.querySelector(".density-head")?.textContent).toEqual("compact");
  reported.mockRestore();
});

test("a throw in an island among the children reaches the boundary around it and a retry renders it again", async () => {
  const reported = spyOn(console, "error").mockImplementation(() => {});
  await load("/planner", { ctx: day() });
  await settle();
  await fireEvent.click(document.querySelector(".reminders") as Element);
  await settle();
  expect(document.querySelector(".planner-failed"), "the boundary caught it").toBeTruthy();
  await fireEvent.click(document.querySelector(".planner-retry") as Element);
  await settle();
  expect(document.querySelector(".reminders")?.textContent).toEqual("Remind me");
  await fireEvent.click(document.querySelector(".floors-room") as Element);
  await settle();
  expect(document.querySelector(".floors-talks li")?.textContent, "the Vue island the retry rendered again mounted").toEqual("09:00 Opening");
  reported.mockRestore();
});

test("an island among the children that suspends shows the fallback around it until it resolves", async () => {
  await load("/planner", { ctx: day() });
  await settle();
  await fireEvent.click(document.querySelector(".export") as Element);
  await settle();
  expect(document.querySelector(".planner-loading"), "the boundary's fallback").toBeTruthy();
  (globalThis as { finishPlannerExport?: () => void }).finishPlannerExport?.();
  await advance(1000);
  expect(document.querySelector(".planner-loading")).toBeNull();
  expect(document.querySelector(".export")?.textContent).toEqual("Exported");
});

test("a Vue island inside the React tree mounts as a root of its own", async () => {
  await load("/planner", { ctx: day() });
  await settle();
  const floors = document.querySelector('sf-i[data-sf-module="src/ui/Floors.vue#default"]');
  expect(floors?.hasAttribute("data-sf-mounted"), "Vue's adapter mounted it").toBe(true);
  await fireEvent.click(document.querySelector(".floors-room") as Element);
  await settle();
  expect(document.querySelector(".floors-talks li")?.textContent).toEqual("09:00 Opening");
});

test("an island timed on visible keeps the server's markup in the tree until it is shown", async () => {
  await load("/planner", { ctx: day() });
  await settle();
  const later = document.querySelector(".planner-later");
  expect(later?.textContent, "the server rendered it in the shell's frame").toEqual("comfortable");
  expect(later?.parentElement?.firstChild?.nodeType, "bracketed as a boundary of its own").toEqual(8);
});
