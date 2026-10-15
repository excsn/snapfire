import { bindHtmx } from "@snapfire/fsr-client/htmx";
import { ctx, expect, load, settle, test } from "@snapfire/fsr-client/testing";

const conference = { name: "Everything At Once", day: "Thursday 18 September", venue: "The Old Exchange", tracks: ["Runtime", "Data"] };

const day = () =>
  ctx({
    services: {
      program: { getConference: () => conference, listTalks: () => [], listAnnouncements: () => [], listSponsors: () => [] },
    },
  });

test("htmx leaves a step back to the navigator", async () => {
  await load("/", { ctx: day() });
  await settle();
  const unbind = bindHtmx({ process() {} });
  // htmx 4 fires this on a step back to an entry it stamped, then fetches the page and swaps the body.
  const restore = new CustomEvent("htmx:before:history:restore", { cancelable: true, detail: { path: "/" } });
  document.dispatchEvent(restore);
  unbind();
  expect(restore.defaultPrevented).toEqual(true);
});
