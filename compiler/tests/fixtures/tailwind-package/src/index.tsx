const cn = (...parts: (string | false)[]) => parts.filter(Boolean).join(" ");

export function Badge({ tone, children }: { tone: "accent" | "plain"; children: string }) {
  return <span className={cn("rounded-full px-2", tone === "accent" ? "bg-violet-100 text-violet-800" : "bg-gray-100")}>{children}</span>;
}
