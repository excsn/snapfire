import type { Ctx } from "@snapfire/fsr";

const photo = (name: string, alt: string) => ({
  alt,
  fallback: `/static/img/${name}-960.jpg`,
  width: 960,
  height: 640,
  sources: [
    { url: `/static/img/${name}-1440.avif`, width: 1200, type: "image/avif" },
    { url: `/static/img/${name}-960.avif`, width: 720, type: "image/avif" },
    { url: `/static/img/${name}-640.avif`, width: 0, type: "image/avif" },
  ],
});

export async function load(_ctx: Ctx<"/lowered">) {
  return {
    hero: {
      heading: "Field notes, kept in one place",
      body: "A notebook app for people who work outdoors: offline first, synced when you are back in range and readable in direct sunlight.",
      media: photo("hero", "A notebook open on a rock beside a trail map"),
      cta: { label: "Start a notebook", href: "/signup" },
    },
    panels: [
      {
        id: "capture",
        heading: "Capture without signal",
        body: "Every entry is written to the device first. Photos, sketches and GPS fixes queue until a connection comes back.",
        media: photo("capture", "A phone showing a sketch with a location pin"),
        points: ["Works in airplane mode", "Queues photos at full resolution", "Stamps each entry with time and position"],
        reverse: false,
        cta: { label: "How sync works", href: "/docs/sync" },
      },
      {
        id: "share",
        heading: "Share a page, not the notebook",
        body: "Send one entry to a colleague as a link that expires, with the location blurred to the precision you choose.",
        media: photo("share", "Two people looking at a shared entry on a tablet"),
        points: ["Links expire after a day, a week or never", "Location precision from exact to region"],
        reverse: true,
        footnote: "Expiring links need a team plan.",
      },
      {
        id: "export",
        heading: "Export when the season ends",
        body: "Download a season as a PDF, a CSV of entries or GeoJSON of every fix.",
        media: photo("export", "A printed field report beside a laptop"),
        points: [],
        reverse: false,
        cta: { label: "See export formats", href: "/docs/export" },
        footnote: "GeoJSON export keeps the original coordinates.",
      },
    ],
    terms: [
      {
        type: "paragraph",
        children: [
          { type: "text", text: "Plans are billed " },
          { type: "text", text: "monthly", bold: true },
          { type: "text", text: " and can be cancelled at any time. See the " },
          { type: "link", href: "/pricing", children: [{ type: "text", text: "pricing page" }] },
          { type: "text", text: " for what each plan includes." },
        ],
      },
      {
        type: "list",
        ordered: false,
        children: [
          { type: "item", children: [{ type: "text", text: "Personal: one notebook, unlimited entries" }] },
          { type: "item", children: [{ type: "text", text: "Team: shared notebooks and expiring links" }] },
          { type: "item", children: [{ type: "text", text: "Field office: everything in Team plus an audit log" }] },
        ],
      },
      {
        type: "paragraph",
        children: [
          { type: "text", text: "Questions about data residency?", italic: true },
          { type: "linebreak" },
          { type: "link", href: "https://example.com/privacy", external: true, children: [{ type: "text", text: "Read the privacy notice" }] },
        ],
      },
    ],
  };
}
