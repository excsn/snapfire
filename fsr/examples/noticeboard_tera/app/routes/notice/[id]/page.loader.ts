import type { Ctx } from "@snapfire/fsr";
import { modified } from "@snapfire/fsr/head";
import { notices } from "@src/content";

export async function load({ params }: Ctx<"/notice/{id}">) {
  return { notice: notices.find((n) => n.id === params.id) ?? null };
}

export const meta = ({ data }: { data: { notice: { title: string; posted: string } | null } }) => ({
  title: data.notice ? data.notice.title : "No such notice",
  head: data.notice ? [modified(data.notice.posted)] : [],
});

export const paths = () => notices.map((n) => ({ id: n.id }));
