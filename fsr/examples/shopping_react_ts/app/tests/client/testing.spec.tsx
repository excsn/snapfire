import { useState } from "react";
import { Link } from "@snapfire/fsr-client/react";
import { afterEach, beforeAll, beforeEach, describe, expect, fireEvent, fn, it, render, screen, spyOn, test, userEvent, vi, within } from "@snapfire/fsr-client/testing";

const order: string[] = [];

describe("hooks", () => {
  beforeAll(() => {
    order.push("beforeAll");
  });
  beforeEach(() => {
    order.push("beforeEach");
  });
  afterEach(() => {
    order.push("afterEach");
  });

  it("runs the block's hooks around each test", () => {
    expect(order).toEqual(["beforeAll", "beforeEach"]);
  });

  describe("nested", () => {
    beforeEach(() => {
      order.push("inner");
    });

    it("runs the outer hooks before the inner ones", () => {
      expect(order.slice(-2)).toEqual(["beforeEach", "inner"]);
    });
  });
});

test.each([
  [1, 2, 3],
  [2, 3, 5],
])("%i + %i is %i", (a, b, sum) => {
  expect(a + b).toBe(sum);
});

test.each`
  word          | length
  ${"leek"}     | ${4}
  ${"potatoes"} | ${8}
`("$word has $length letters", ({ word, length }) => {
  expect(word).toHaveLength(length);
});

test.skip("a skipped test is reported and never runs", () => {
  throw new Error("ran");
});

test.todo("a todo is reported without a body");

it("waits for its done callback", (done) => {
  queueMicrotask(() => done());
});

test.fails("a test expected to fail passes when it throws", () => {
  expect(1).toBe(2);
});

describe("expect", () => {
  it("compares by value, with an integer read back as a bigint equal to the number it stands for", () => {
    expect({ a: 1, b: [2] }).toEqual({ a: 1, b: [2] });
    expect({ a: 1, b: undefined }).toEqual({ a: 1 });
    expect({ a: 1, b: undefined }).not.toStrictEqual({ a: 1 });
    expect(3n).toBe(3);
    expect([1, 2, 3]).toContain(2);
    expect([{ id: 1 }]).toContainEqual({ id: 1 });
    expect("leek and potato").toMatch(/potato$/);
    expect({ id: 1, name: "x", extra: true }).toMatchObject({ id: 1 });
    expect({ a: { b: [1, { c: 2 }] } }).toHaveProperty("a.b[1].c", 2);
    expect(0.1 + 0.2).toBeCloseTo(0.3);
    expect(null).toBeNull();
    expect(undefined).toBeUndefined();
    expect(NaN).toBeNaN();
    expect(5).toBeGreaterThan(4);
    expect(5).toBeLessThanOrEqual(5);
    expect("x").toBeTypeOf("string");
  });

  it("reads the helpers that match a kind of value inside one", () => {
    expect({ id: 7, at: "09:10", tags: ["a", "b"] }).toEqual({ id: expect.any(Number), at: expect.stringMatching(/^\d\d:\d\d$/), tags: expect.arrayContaining(["b"]) });
    expect({ id: 7, name: "x" }).toEqual(expect.objectContaining({ id: 7 }));
    expect("abc").toEqual(expect.not.stringContaining("z"));
  });

  it("says what was expected and what arrived", () => {
    expect(() => expect(1).toBe(2)).toThrow("Expected: 2");
    expect(() => expect([1]).not.toContain(1)).toThrow(/not\.toContain/);
  });

  it("awaits a promise with resolves and rejects", async () => {
    await expect(Promise.resolve(4)).resolves.toBe(4);
    await expect(Promise.reject(new Error("nope"))).rejects.toThrow("nope");
    await expect(() => expect(Promise.resolve(1)).rejects.toThrow()).rejects.toThrow(/resolved instead of rejecting/);
  });

  it("counts the assertions a test asked for", () => {
    expect.assertions(2);
    expect(1).toBe(1);
    expect(2).toBe(2);
  });
});

describe("mock functions", () => {
  it("record their calls and answer what they are told", () => {
    const add = fn((a: number, b: number) => a + b);
    expect(add(1, 2)).toBe(3);
    add.mockReturnValueOnce(10);
    expect(add(1, 1)).toBe(10);
    expect(add).toHaveBeenCalledTimes(2);
    expect(add).toHaveBeenCalledWith(1, 2);
    expect(add).toHaveBeenLastCalledWith(1, 1);
    expect(add).toHaveReturnedWith(3);
    expect(vi.isMockFunction(add)).toBe(true);
  });

  it("spy on a method and put it back", () => {
    const shelf = { count: (n: number) => n * 2 };
    const spy = spyOn(shelf, "count");
    expect(shelf.count(4)).toBe(8);
    expect(spy).toHaveBeenCalledOnce();
    spy.mockImplementation(() => 0);
    expect(shelf.count(4)).toBe(0);
    spy.mockRestore();
    expect(shelf.count(4)).toBe(8);
    expect(vi.isMockFunction(shelf.count)).toBe(false);
  });
});

function Pantry() {
  const [items, setItems] = useState<string[]>([]);
  const [name, setName] = useState("");
  const [saved, setSaved] = useState(false);
  return (
    <form
      aria-label="Pantry"
      onSubmit={(event) => {
        event.preventDefault();
        if (!name) return;
        setItems([...items, name]);
        setName("");
        setTimeout(() => setSaved(true), 200);
      }}
    >
      <label>
        Item <input value={name} onChange={(event) => setName(event.target.value)} />
      </label>
      <label>
        <input type="checkbox" /> Organic
      </label>
      <select aria-label="Aisle" defaultValue="dry">
        <option value="dry">Dry goods</option>
        <option value="cold">Cold</option>
      </select>
      <button type="submit">Add</button>
      <ul aria-label="Items">
        {items.map((item) => (
          <li key={item}>{item}</li>
        ))}
      </ul>
      {saved ? <p role="status">Saved</p> : null}
      <h2>Shelf</h2>
    </form>
  );
}

describe("queries and a user", () => {
  it("finds a control by its role and name, types into it and submits with the button", async () => {
    await render(<Pantry />);
    const user = userEvent.setup();
    const input = screen.getByRole("textbox", { name: "Item" });
    await user.type(input, "leeks");
    expect(input).toHaveValue("leeks");
    expect(input).toHaveFocus();
    await user.click(screen.getByRole("button", { name: "Add" }));
    const list = screen.getByRole("list", { name: "Items" });
    expect(within(list).getAllByRole("listitem").map((li) => li.textContent)).toEqual(["leeks"]);
    expect(input).toHaveValue("");
    expect(screen.getByRole("heading", { level: 2 })).toHaveTextContent("Shelf");
  });

  it("toggles a checkbox through its label and picks an option", async () => {
    await render(<Pantry />);
    const user = userEvent.setup();
    const box = screen.getByRole("checkbox", { name: "Organic" });
    expect(box).not.toBeChecked();
    await user.click(screen.getByText("Organic"));
    expect(box).toBeChecked();
    const aisle = screen.getByRole("combobox", { name: "Aisle" });
    await user.selectOptions(aisle, "cold");
    expect(aisle).toHaveValue("cold");
    expect(aisle).toHaveDisplayValue("Cold");
  });

  it("waits for what arrives later, moving the clock as it waits", async () => {
    await render(<Pantry />);
    const user = userEvent.setup();
    await user.type(screen.getByLabelText("Item"), "stock{Enter}");
    expect(screen.queryByRole("status")).toBeNull();
    expect(await screen.findByRole("status")).toHaveTextContent("Saved");
  });

  it("changes a value through an event's target", async () => {
    await render(<Pantry />);
    const input = screen.getByRole("textbox", { name: "Item" }) as HTMLInputElement;
    await fireEvent.change(input, { target: { value: "oats" } });
    expect(input.value).toBe("oats");
  });

  it("lists the roles that were there when a role query finds none", async () => {
    await render(<Pantry />);
    expect(() => screen.getByRole("dialog")).toThrow(/Accessible roles here/);
    expect(screen.queryAllByRole("listitem")).toHaveLength(0);
  });

  it("reads the markup the way a reader meets it", async () => {
    const r = await render(
      <p className="note lead" data-kind="tip">
        Hello <button disabled>Go</button>
      </p>,
    );
    const note = r.container.querySelector("p") as HTMLElement;
    expect(note).toBeInTheDocument();
    expect(note).toHaveClass("note");
    expect(note).toHaveClass("lead note", { exact: true });
    expect(note).toHaveAttribute("data-kind", "tip");
    expect(note).toBeVisible();
    expect(r.getByRole("button", { name: "Go" })).toBeDisabled();
    r.unmount();
    expect(note).not.toBeInTheDocument();
  });
});

test("a node reads as its interface, the way a browser names it", () => {
  expect(Object.prototype.toString.call(document.createElement("div"))).toMatch(/^\[object HTML\w*Element\]$/);
  expect(Object.prototype.toString.call(document.createTextNode("x"))).toEqual("[object Text]");
});

test("an attribute name on an HTML element is matched in any case, the way a browser matches it", () => {
  document.body.innerHTML = '<input autocomplete="off"><svg viewBox="0 0 1 1"></svg>';
  const input = document.querySelector("input") as HTMLInputElement;
  expect(input.getAttribute("autoComplete"), "React's spelling finds the attribute the markup wrote").toEqual("off");
  input.setAttribute("tabIndex", "2");
  expect(input.getAttribute("tabindex"), "a name is written in lower case").toEqual("2");
  input.removeAttribute("autoComplete");
  expect(input.hasAttribute("autocomplete"), "and removed in any case").toBeFalsy();
  expect(document.querySelector("svg")?.getAttribute("viewBox"), "an SVG element's name is taken as written").toEqual("0 0 1 1");
});

test("a button carries its value and a control names the form it belongs to", () => {
  document.body.innerHTML = '<form id="f"><button value="Friday" name="answer">Fri</button><input name="q"></form><select form="f"></select><textarea></textarea>';
  const form = document.getElementById("f");
  const button = document.querySelector("button") as HTMLButtonElement;
  expect(button.value, "a button's value is its attribute").toEqual("Friday");
  expect(button.form, "a control inside a form belongs to it").toBe(form);
  expect((document.querySelector("input") as HTMLInputElement).form).toBe(form);
  expect((document.querySelector("select") as HTMLSelectElement).form, "one outside it belongs to the form its attribute names").toBe(form);
  expect((document.querySelector("textarea") as HTMLTextAreaElement).form, "and one in no form belongs to none").toBeNull();
});

test("a link writes the keep it was given for the navigator to read", async () => {
  const r = await render(
    <p>
      <Link href="/a" keep={false}>
        not kept
      </Link>
      <Link href="/b" keep>
        kept
      </Link>
      <Link href="/c">left to the navigator</Link>
    </p>,
  );
  const keeps = ["not kept", "kept", "left to the navigator"].map((text) => screen.getByText(text).getAttribute("data-sf-keep"));
  expect(keeps).toEqual(["false", "true", null]);
  r.unmount();
});
