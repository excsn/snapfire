import { expect, fireEvent, load, screen, settle, test } from "@snapfire/fsr-client/testing";

test("the books opened lately come from a runes module the progress island writes", async () => {
  await load("/");
  await settle();
  expect(document.querySelector(".recent .none")?.textContent).toEqual("Nothing yet.");

  await fireEvent.click(screen.getByText("Piranesi"));
  await settle();
  await fireEvent.click(screen.getByText("All books"));
  await settle();
  await fireEvent.click(screen.getByText("Stoner"));
  await settle();

  const opened = Array.from(document.querySelectorAll(".recent li")).map((li) => li.textContent);
  expect(opened).toEqual(["Stoner", "Piranesi"]);
});
