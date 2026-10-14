export type Card = { title: string; body: string };
export type Step = { command: string; explains: string };

export const cards: Card[] = [
  { title: "Routes are files", body: "A directory under routes/ is a path, page.tsx is what renders and page.loader.ts is the data it renders from." },
  { title: "Components render in Rust", body: "A page lowers to the IR, so the server needs no JavaScript engine to answer a request." },
  { title: "Islands hydrate", body: "Anything that needs the browser says so, and only that mounts." },
];

export const steps: Step[] = [
  { command: "fsr new handbook --with react", explains: "writes the project, fetches the client and vendors React; without the flag it writes a bare application." },
  { command: "fsr dev app", explains: "watches, rebuilds and refreshes the open page." },
  { command: "fsr build app", explains: "emits the plan, the contracts and the browser bundle." },
  { command: "fsr prerender app", explains: "renders every fixed route to a file, which is all this site is." },
];
