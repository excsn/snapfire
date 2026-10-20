import { Island } from "@snapfire/fsr-authoring/template";
import { Link } from "@snapfire/fsr/components";

import Progress from "@src/ui/Progress.svelte";
import type { BookIdProps } from "@generated/client";

export default function BookPage({ book, read, done, current }: BookIdProps) {
  return (
    <article className="book-page">
      <p className="back">
        <Link href="/">All books</Link>
      </p>
      <h1>{book.title}</h1>
      <p className="by">{book.author}</p>
      <Island when="load">
        <Progress id={book.id} pages={book.pages} read={read} done={done} current={current} />
      </Island>
    </article>
  );
}
