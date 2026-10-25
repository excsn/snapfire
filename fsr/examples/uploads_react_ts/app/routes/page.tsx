import { useRef, useState } from "react";
import { upload } from "@snapfire/fsr/client";
import { Island } from "@snapfire/fsr/client/react";
import type { RootProps } from "@generated/client";

export default function Page({
  held,
  error,
  csrf_token,
  action_failure,
}: RootProps) {
  return (
    <main className="mx-auto max-w-4xl px-4 py-12 sm:px-6">
      <header className="mb-8">
        <h1 className="m-0 text-3xl font-bold tracking-tight text-white">Uploads</h1>
        <p className="mt-3 max-w-2xl text-base leading-relaxed text-zinc-400">
        One action, two callers. The first form posts natively, so it works with JavaScript switched off. The second
        posts the same action as a <code className="rounded bg-zinc-800 px-1.5 py-0.5 text-sm text-zinc-200">FormData</code>, which is the same wire with a different caller.
        </p>
      </header>

      {error ? <p className="mb-6 rounded-lg border border-red-500/40 bg-red-500/10 px-4 py-3 text-sm text-red-300">{error}</p> : null}
      {action_failure ? (
        <p className="mb-6 rounded-lg border border-red-500/40 bg-red-500/10 px-4 py-3 text-sm text-red-300">
          {action_failure.kind}: {action_failure.message}
        </p>
      ) : null}

      <div className="grid gap-6 md:grid-cols-2">
      <section className="rounded-2xl border border-zinc-800 bg-zinc-900/60 p-6 shadow-lg shadow-black/20">
        <h2 className="m-0 text-xs font-semibold uppercase tracking-widest text-zinc-400">Without JavaScript</h2>
        <form className="mt-5 space-y-4" method="post" action="/_sf/action/$root.deposit" encType="multipart/form-data">
          <input type="hidden" name="_csrf" value={csrf_token ?? ""} />
          <label className="block text-sm font-medium text-zinc-300">
            Caption
            <input className="mt-1.5 block w-full rounded-lg border border-zinc-700 bg-zinc-950 px-3 py-2 text-sm text-zinc-100 placeholder-zinc-500 outline-none focus:border-sky-500 focus:ring-2 focus:ring-sky-500/30" name="caption" defaultValue="" placeholder="What is it?" />
          </label>
          <label className="block text-sm font-medium text-zinc-300">
            File
            <input className="mt-1.5 block w-full text-sm text-zinc-400 file:mr-3 file:cursor-pointer file:rounded-md file:border-0 file:bg-zinc-800 file:px-3 file:py-2 file:text-sm file:font-medium file:text-zinc-100 hover:file:bg-zinc-700" type="file" name="file" />
          </label>
          <button className="inline-flex cursor-pointer items-center rounded-lg border-0 bg-sky-500 px-4 py-2 text-sm font-semibold text-white shadow-sm hover:bg-sky-400 focus:outline-none focus:ring-2 focus:ring-sky-500/40" type="submit">Deposit</button>
        </form>
      </section>

      <section className="rounded-2xl border border-zinc-800 bg-zinc-900/60 p-6 shadow-lg shadow-black/20">
        <h2 className="m-0 text-xs font-semibold uppercase tracking-widest text-zinc-400">With it</h2>
        <Island when="load">
          <Deposit token={csrf_token ?? ""} />
        </Island>
      </section>
      </div>

      <section className="mt-6 rounded-2xl border border-zinc-800 bg-zinc-900/60 p-6 shadow-lg shadow-black/20">
        <div className="flex items-center justify-between gap-4">
          <h2 className="m-0 text-xs font-semibold uppercase tracking-widest text-zinc-400">Held this session</h2>
          <form method="post" action="/_sf/action/$root.clear">
            <input type="hidden" name="_csrf" value={csrf_token ?? ""} />
            <button className="cursor-pointer rounded-lg border border-zinc-700 bg-transparent px-3 py-1.5 text-sm font-medium text-zinc-300 hover:border-zinc-500 hover:text-white" type="submit">Clear</button>
          </form>
        </div>
        {held.length === 0 ? (
          <p className="mt-5 mb-0 rounded-xl border border-dashed border-zinc-700 px-4 py-8 text-center text-sm text-zinc-500">Nothing yet. Deposit a file above.</p>
        ) : (
          <div className="mt-5 overflow-x-auto rounded-xl border border-zinc-800">
            <table className="w-full border-collapse text-left text-sm">
              <thead className="bg-zinc-900 text-xs uppercase tracking-wider text-zinc-400">
                <tr>
                  <th className="px-4 py-3 font-semibold">File</th>
                  <th className="px-4 py-3 font-semibold">Type</th>
                  <th className="px-4 py-3 text-right font-semibold">Bytes</th>
                  <th className="px-4 py-3 font-semibold">Caption</th>
                </tr>
              </thead>
              <tbody>
                {held.map((row) => (
                  <tr className="border-t border-zinc-800 hover:bg-zinc-800/40" key={`${row.filename}-${String(row.size)}`}>
                    <td className="max-w-xs truncate px-4 py-3 font-medium text-zinc-100">{row.filename}</td>
                    <td className="px-4 py-3 text-zinc-400">{row.content_type}</td>
                    <td className="px-4 py-3 text-right tabular-nums text-zinc-400">{String(row.size)}</td>
                    <td className="px-4 py-3 text-zinc-300">{row.caption}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </section>
      <p className="mt-8">
        <a className="text-sm font-medium text-sky-400 no-underline hover:text-sky-300 hover:underline" href="/tour">A page with no form →</a>
      </p>
    </main>
  );
}

export function Deposit({ token }: { token: string }) {
  const form = useRef<HTMLFormElement>(null);
  const [state, setState] = useState("");

  async function send(event: React.FormEvent) {
    event.preventDefault();
    if (!form.current) return;
    const data = new FormData(form.current);
    data.set("_csrf", token);
    setState("posting");
    try {
      await upload("$root.deposit", data);
      setState("done");
    } catch (e) {
      setState(e instanceof Error ? e.message : "failed");
    }
  }

  return (
    <form className="mt-5 space-y-4" ref={form} onSubmit={send}>
      <label className="block text-sm font-medium text-zinc-300">
        Caption
        <input className="mt-1.5 block w-full rounded-lg border border-zinc-700 bg-zinc-950 px-3 py-2 text-sm text-zinc-100 placeholder-zinc-500 outline-none focus:border-sky-500 focus:ring-2 focus:ring-sky-500/30" name="caption" defaultValue="" placeholder="What is it?" />
      </label>
      <label className="block text-sm font-medium text-zinc-300">
        File
        <input className="mt-1.5 block w-full text-sm text-zinc-400 file:mr-3 file:cursor-pointer file:rounded-md file:border-0 file:bg-zinc-800 file:px-3 file:py-2 file:text-sm file:font-medium file:text-zinc-100 hover:file:bg-zinc-700" type="file" name="file" />
      </label>
      <div className="flex items-center gap-3">
        <button className="inline-flex cursor-pointer items-center rounded-lg border-0 bg-sky-500 px-4 py-2 text-sm font-semibold text-white shadow-sm hover:bg-sky-400 focus:outline-none focus:ring-2 focus:ring-sky-500/40" type="submit">Deposit</button>
        {state ? <span className="rounded-full bg-zinc-800 px-2.5 py-1 text-xs font-medium text-zinc-300">{state}</span> : null}
      </div>
    </form>
  );
}
