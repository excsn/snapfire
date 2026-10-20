export default function Missing({ error }: { error: string }) {
  return (
    <main className="mx-auto max-w-4xl px-4 py-12 sm:px-6">
      <h1 className="m-0 mb-4 text-3xl font-bold tracking-tight text-white">Nothing here</h1>
      <p className="rounded-lg border border-red-500/40 bg-red-500/10 px-4 py-3 text-sm text-red-300">{error}</p>
      <a className="mt-6 inline-block text-sm font-medium text-sky-400 no-underline hover:text-sky-300 hover:underline" href="/">← Back</a>
    </main>
  );
}
