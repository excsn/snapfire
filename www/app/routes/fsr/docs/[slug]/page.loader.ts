import type { Ctx } from "@snapfire/fsr";
import { canonical, og } from "@snapfire/fsr/head";
import { CHAPTERS } from "@src/docs/guide";

export async function load({ params }: Ctx<"/fsr/docs/{slug}">) {
  const at = CHAPTERS.findIndex((c) => c.slug === params.slug);
  const index = at < 0 ? 0 : at;
  const prev = index > 0 ? CHAPTERS[index - 1] : null;
  const next = index < CHAPTERS.length - 1 ? CHAPTERS[index + 1] : null;

  return {
    chapter: CHAPTERS[index],
    prev: prev ? { slug: prev.slug, title: `${prev.number}. ${prev.title}` } : null,
    next: next ? { slug: next.slug, title: `${next.number}. ${next.title}` } : null,
    allChapters: CHAPTERS.map((c) => ({ slug: c.slug, number: c.number, title: c.title, section: c.section })),
  };
}

export const meta = ({ data }: { data: { chapter: { slug: string; number: string; title: string; audience: string } } }) => ({
  title: `${data.chapter.number}. ${data.chapter.title} · The FSR guide`,
  description: `Chapter ${data.chapter.number} of the SnapFire FSR guide. For ${data.chapter.audience}.`,
  head: [
    og("type", "article"),
    og("title", `${data.chapter.number}. ${data.chapter.title}`),
    canonical(`/fsr/docs/${data.chapter.slug}`),
  ],
});
