import { ctx, expect, fireEvent, load, screen, settle, test } from "@snapfire/fsr-client/testing";

test("reading ten more pages saves them through the action and moves the bar", async () => {
  const c = ctx({ session: { reading: "", progress: { piranesi: 20 }, done: {} } });
  await load("/book/piranesi", { ctx: c });
  expect(document.querySelector(".progress .page")?.textContent).toEqual("20");

  await fireEvent.click(screen.getByText("Read 10 more"));
  await settle();

  expect(c.session.progress).toEqual({ piranesi: 30 });
  expect(document.querySelector(".progress .page")?.textContent).toEqual("30");
  expect(document.querySelector(".progress .percent")?.textContent).toEqual("11%");
});

test("finishing a book moves the count in the masthead, which a legacy component reads", async () => {
  const c = ctx({ session: { reading: "", progress: {}, done: { stoner: true } } });
  await load("/book/gilead", { ctx: c });
  expect(document.querySelector(".tally .done")?.textContent).toEqual("1");
  expect(document.querySelector(".tally .left")?.textContent).toEqual("5");

  await fireEvent.click(screen.getByText("Finish"));
  await settle();

  expect(c.session.done).toEqual({ stoner: true, gilead: true });
  expect(screen.getByText("Finished.")).toBeTruthy();
  expect(document.querySelector(".tally .done")?.textContent).toEqual("2");
  expect(document.querySelector(".tally .left")?.textContent).toEqual("4");
  expect((screen.getByText("Finish") as HTMLButtonElement).disabled).toBe(true);
});

test("starting a finished book again takes one off the count", async () => {
  const c = ctx({ session: { reading: "", progress: { stoner: 278 }, done: { stoner: true } } });
  await load("/book/stoner", { ctx: c });
  await fireEvent.click(screen.getByText("Start again"));
  await settle();
  expect(c.session.done).toEqual({ stoner: false });
  expect(document.querySelector(".tally .done")?.textContent).toEqual("0");
});

test("picking the book being read writes the session and the button goes", async () => {
  const c = ctx({ session: { reading: "", progress: {}, done: {} } });
  await load("/book/remains", { ctx: c });
  await fireEvent.click(screen.getByText("Reading this now"));
  await settle();
  expect(c.session.reading).toEqual("remains");
  expect(screen.queryByText("Reading this now")).toBeNull();
});
