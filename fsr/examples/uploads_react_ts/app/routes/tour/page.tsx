import { Embed } from "@snapfire/fsr/components";
import type { TourProps } from "@generated/client";

export default function Tour({ video }: TourProps) {
  return (
    <main className="mx-auto max-w-4xl px-4 py-12 sm:px-6">
      <h1 className="m-0 text-3xl font-bold tracking-tight text-white">Tour</h1>
      <p className="mt-3 max-w-2xl text-base leading-relaxed text-zinc-400">
        This page renders no form, so it reads no CSRF token and starts no session, even under{" "}
        <code className="rounded bg-zinc-800 px-1.5 py-0.5 text-sm text-zinc-200">csrf = "always"</code>. The video id comes from the loader.
      </p>
      <div className="mt-8 overflow-hidden rounded-2xl border border-zinc-800 shadow-lg shadow-black/20">
        <Embed provider="youtube" id={video.id} title={video.title} />
      </div>
      <p className="mt-8">
        <a className="text-sm font-medium text-sky-400 no-underline hover:text-sky-300 hover:underline" href="/">← Back to the uploads</a>
      </p>
    </main>
  );
}
