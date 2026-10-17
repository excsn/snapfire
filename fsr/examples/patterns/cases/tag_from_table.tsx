const levels = { title: { Tag: "h1", cls: "big" }, section: { Tag: "h2", cls: "mid" } } as const;
export default function Page() {
  const { Tag, cls } = levels["section" as keyof typeof levels];
  return <Tag className={cls}>Heading</Tag>;
}
