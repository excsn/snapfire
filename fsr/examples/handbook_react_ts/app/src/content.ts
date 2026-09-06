export type Card = { title: string; body: string };
export type Step = { command: string; explains: string };
export type Question = { asks: string; answers: string };

export const cards: Card[] = [
  { title: "Routes are files", body: "A directory under routes/ is a path, page.tsx is what renders and page.loader.ts is the data it renders from." },
  { title: "Components render in Rust", body: "A page lowers to the IR, so the server needs no JavaScript engine to answer a request." },
  { title: "Islands hydrate", body: "Anything that needs the browser says so, and only that mounts." },
];

export const steps: Step[] = [
  { command: "fsr new handbook", explains: "writes the project, fetches the client and vendors React." },
  { command: "fsr dev app", explains: "watches, rebuilds and refreshes the open page." },
  { command: "fsr build app", explains: "emits the plan, the contracts and the browser bundle." },
  { command: "fsr prerender app", explains: "renders every fixed route to a file, which is all this site is." },
];

export const questions: Question[] = [
  { asks: "Does this site run a server?", answers: "No. Its binary renders every route to a file and exits; a static host serves the directory." },
  { asks: "What makes a route prerenderable?", answers: "No parameter in the pattern, and nothing on the plan reads the request, the session or an identity." },
  { asks: "Where does the JavaScript come from?", answers: "The bundle the build wrote, plus vendored React. Nothing is fetched at runtime." },
];
