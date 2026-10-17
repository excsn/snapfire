import { Activity, Suspense } from "react";

import Adder from "@src/idioms/Adder";
import Ager from "@src/idioms/Ager";
import Board from "@src/idioms/Board";
import Clock from "@src/idioms/Clock";
import Tasks from "@src/idioms/Tasks";
import Typed from "@src/idioms/Typed";

const levels = { 1: "One", 2: "Two" };
const stats = new Map([
  ["alice", 100],
  ["bob", 200],
]);
const tags = new Set(["react", "next"]);
const when = new Date("2025-06-15T12:00:00Z");
const big = 9007199254740993n;

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
      <Ager name="Ada" />
      <Typed map={stats} set={tags} date={when} big={big} />
      <Clock total={1234.5} />
    </section>
  );
}
