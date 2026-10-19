import { pick, progress } from "@routes/book/[id]/actions";
import { ctx, expect, test } from "@snapfire/fsr/testing";

test("saving pages writes the session and answers the page and the finished count", async () => {
  const c = ctx<{ book_id: string; page: number }>({ input: { book_id: "piranesi", page: 40 }, session: { reading: "", progress: {}, done: {} } });
  const r = await progress(c);
  expect(r).toEqual({ page: 40, finished: false, done: 0 });
  expect(c.session.progress).toEqual({ piranesi: 40 });
  expect(c.session.done).toEqual({ piranesi: false });
});

test("a page past the end is held at the last page and finishes the book", async () => {
  const c = ctx<{ book_id: string; page: number }>({ input: { book_id: "earthsea", page: 500 }, session: { reading: "", progress: {}, done: { stoner: true } } });
  const r = await progress(c);
  expect(r).toEqual({ page: 183, finished: true, done: 2 });
});

test("starting a finished book again takes it off the finished count", async () => {
  const c = ctx<{ book_id: string; page: number }>({ input: { book_id: "earthsea", page: 0 }, session: { reading: "", progress: { earthsea: 183 }, done: { earthsea: true } } });
  const r = await progress(c);
  expect(r).toEqual({ page: 0, finished: false, done: 0 });
});

test("a book the shelf does not hold or a negative page is refused and the session is untouched", async () => {
  const unknown = ctx<{ book_id: string; page: number }>({ input: { book_id: "dune", page: 10 }, session: { reading: "", progress: {}, done: {} } });
  await expect(progress(unknown)).rejects.toMatchObject({ kind: "not_found" });
  expect(unknown.session.progress).toEqual({});
  const negative = ctx<{ book_id: string; page: number }>({ input: { book_id: "stoner", page: -1 }, session: { reading: "", progress: {}, done: {} } });
  await expect(progress(negative)).rejects.toMatchObject({ kind: "invalid" });
  expect(negative.session.progress).toEqual({});
});

test("picking a book makes it the one being read", async () => {
  const c = ctx<{ book_id: string }>({ input: { book_id: "gilead" }, session: { reading: "", progress: {}, done: {} } });
  expect(await pick(c)).toEqual({ reading: "gilead" });
  expect(c.session.reading).toEqual("gilead");
  const unknown = ctx<{ book_id: string }>({ input: { book_id: "dune" }, session: { reading: "", progress: {}, done: {} } });
  await expect(pick(unknown)).rejects.toMatchObject({ kind: "not_found" });
  expect(unknown.session.reading).toEqual("");
});
