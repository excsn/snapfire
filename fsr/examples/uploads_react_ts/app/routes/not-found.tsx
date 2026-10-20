export default function NotFound({ params }: { params: { path: string } }) {
  return (
    <main className="mx-auto max-w-4xl px-4 py-12 sm:px-6">
      <h1 className="m-0 mb-4 text-3xl font-bold tracking-tight text-white">No page at {params.path}</h1>
      <a className="mt-6 inline-block text-sm font-medium text-sky-400 no-underline hover:text-sky-300 hover:underline" href="/">← Back</a>
    </main>
  );
}
