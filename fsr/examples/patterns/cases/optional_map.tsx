export default function Page() {
  const tags: string[] | null = ["new", "hot"];
  const none: string[] | null = null;
  return <div><ul>{tags?.map((t) => <li key={t}>{t}</li>)}</ul><ul>{none?.map((t) => <li key={t}>{t}</li>)}</ul></div>;
}
