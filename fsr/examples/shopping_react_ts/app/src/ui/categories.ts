export const categories: { key: string; label: string }[] = [
  { key: "printing", label: "3D printing" },
  { key: "tools", label: "Tools" },
  { key: "food", label: "Food and drink" },
  { key: "tech", label: "Tech" },
  { key: "books", label: "Books" },
];

export function categoryLabel(key: string): string {
  return categories.find((c) => c.key === key)?.label ?? key;
}
