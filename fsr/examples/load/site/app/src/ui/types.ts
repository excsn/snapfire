export type Media = {
  alt: string;
  fallback: string;
  width: number;
  height: number;
  sources: { url: string; width: number; type: string }[];
};

export type RichNode =
  | { type: "text"; text: string; bold?: boolean; italic?: boolean }
  | { type: "linebreak" }
  | { type: "link"; href: string; external?: boolean; children: RichNode[] }
  | { type: "paragraph"; children: RichNode[] }
  | { type: "list"; ordered: boolean; children: RichNode[] }
  | { type: "item"; children: RichNode[] };
