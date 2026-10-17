import { Activity, Suspense } from "react";

import Adder from "@src/idioms/Adder";
import Board from "@src/idioms/Board";
import Tasks from "@src/idioms/Tasks";

const levels = { 1: "One", 2: "Two" };

function Heading({ level, children }: { level: number; children: string }) {
  switch (level) {
    case 1:
      return <h1>{children}</h1>;
    case 2:
      return <h2>{children}</h2>;
    default:
      throw Error("Unknown level: " + level);
  }
}

export default function Idioms() {
  return (
    <section className="idioms">
      <Heading level={1}>{levels[1]}</Heading>
      <Suspense fallback={<p>loading</p>}>
        <Board />
      </Suspense>
      <Activity mode="visible">
        <Tasks />
      </Activity>
      <Activity mode="hidden">
        <p className="hidden">hidden</p>
      </Activity>
      <Heading level={2}>{levels[2]}</Heading>
      <Adder />
    </section>
  );
}
