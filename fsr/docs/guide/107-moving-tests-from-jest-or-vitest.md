# 107. Moving tests from Jest or Vitest

The question this chapter answers: you have a suite written for Jest or Vitest with Testing Library. What changes when it runs under `fsr test`?

**For:** app developers moving an existing suite.

FSR's testing API is inspired by Jest, Vitest and Testing Library so that a suite written for them moves over with small changes. The names are the ones you use now: `describe`, `it`, `test`, the four hooks, `expect` with its matchers, `fn` and `spyOn`, `screen` with its queries, `within`, `waitFor`, `fireEvent` and `userEvent`. This chapter is the list of what is different, then a file moved end to end.

## The imports

A page spec, `*.spec.tsx`, imports everything from one module:

```tsx
import { describe, expect, it, render, screen, userEvent } from "@snapfire/fsr-client/testing";
```

A body test, `*.test.ts`, imports its helpers from `@snapfire/fsr/testing` and the loader or action under test by alias:

```ts
import { load } from "@routes/cart/page.loader";
import { ctx, describe, expect, fn, it } from "@snapfire/fsr/testing";
```

There is no `vitest`, `@jest/globals`, `@testing-library/react`, `@testing-library/user-event` or `@testing-library/jest-dom` to install. There are no globals either: the names come from the import.

## What carries over unchanged

| You write | Under fsr test |
| --- | --- |
| `describe`, `it`, `test`, `.skip`, `.only`, `.todo`, `.each` with arrays or a template table | the same, with the same name placeholders |
| `beforeAll`, `afterAll`, `beforeEach`, `afterEach` | the same |
| `expect(x).toBe`, `toEqual`, `toStrictEqual`, `toMatchObject`, `toContain`, `toHaveLength`, `toHaveProperty`, `toThrow` and the rest | the same, with `.not`, `.resolves` and `.rejects` |
| `expect.any`, `expect.objectContaining`, `expect.arrayContaining`, `expect.stringMatching` | the same |
| `expect(x, "message")` | the same: the message leads the report |
| `vi.fn()`, `jest.fn()`, `vi.spyOn()`, `mockReturnValue`, `mockResolvedValue`, `mockImplementation` | the same, from `fn` and `spyOn` or from `vi` and `jest` |
| `toHaveBeenCalledWith`, `toHaveBeenCalledTimes` and the other call matchers | the same |
| `screen.getByRole`, `getByText`, `getByLabelText` and every other query in its six forms | the same |
| `within`, `waitFor`, `waitForElementToBeRemoved`, `renderHook`, `act`, `cleanup` | the same |
| `toBeInTheDocument`, `toHaveTextContent`, `toBeVisible`, `toBeDisabled`, `toHaveValue` and the other markup matchers | the same, with no setup file |
| `userEvent.setup()`, `user.click`, `user.type`, `user.keyboard`, `user.selectOptions`, `user.tab` | the same |

## What changes

**`render` and every event are awaited.** `await render(<Page />)`, `await fireEvent.click(button)` and `await user.click(button)`. Each one settles the engine before it resolves: microtasks, the action round trip, the re-render and timers already due. A line after an unawaited click reads the page before the click took effect.

**Services are mocked through `ctx`, not modules.** There is no `vi.mock` or `jest.mock`. A loader or action reaches the world only through its services, so `ctx({ services: { shopping: { listProducts: () => [filament] } } })` is where a mock goes, in a body test and in a page spec. A service method may be a function, a value or a mock function. The contract checks both what a mock is asked and what it answers. A module mock you used to stub a fetch wrapper has no counterpart because the page never calls one: its actions and loaders go through the generated client, which the runner answers.

**The clock is always fake.** Time never passes on its own. `advance(ms)` moves it. `vi.advanceTimersByTime(ms)` is the same call and returns a promise to await. `vi.useFakeTimers()` and `vi.useRealTimers()` change nothing. `waitFor` and every `findBy` query move the clock in steps of 50 while they wait, up to a second.

**There are no snapshots.** `toMatchSnapshot` and `toMatchInlineSnapshot` fail with a message saying so. Assert on the value.

**A body test is a dialect.** A `*.test.ts` is lowered and replayed, not run as JavaScript, so it holds what [103](103-testing-a-body.md) lists: mocks, runs, local values, mock functions, hooks and expectations. A loop, a helper function or a `console.log` in it fails the file with its line. A failure of a run reads as `{ kind, message }`, so `rejects.toMatchObject({ kind: "invalid" })` pins the kind. `toBe` compares the way `toEqual` does, since a value the interpreter returns has no identity. An integer field reads back as a bigint equal to the number a test writes.

**The DOM is not laid out.** Specs run over linkedom. Visibility and the accessibility tree read attributes and inline styles, never a stylesheet, so `toBeVisible` does not see `display: none` from a CSS file. Focus, typed values and a text control's selection are tracked the way a browser tracks them.

**A test gets no `done` in a body test.** A page spec's test may take `done`; a body test's is async and awaits what it runs.

## A file moved

A Vitest spec for the cart page:

```tsx
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import CartPage from "../src/CartPage";

vi.mock("../src/api", () => ({ checkout: vi.fn().mockResolvedValue({ id: 7 }) }));

describe("the cart", () => {
  it("checks out", async () => {
    render(<CartPage lines={lines} />);
    await userEvent.click(screen.getByRole("button", { name: "Check out" }));
    expect(await screen.findByText("Order 7 placed")).toBeInTheDocument();
  });
});
```

The same spec under `fsr test`:

```tsx
import CartPage from "@routes/cart/page";
import { ctx, describe, expect, fn, it, render, screen, userEvent } from "@snapfire/fsr-client/testing";

describe("the cart", () => {
  it("checks out", async () => {
    const placeOrder = fn(() => ({ id: 7n, total_cents: 4800n, lines: [] }));
    const c = ctx({ session: { cart: { "1": 2n } }, services: { shopping: { placeOrder } } });
    await render(<CartPage lines={lines} />, { ctx: c });
    await userEvent.click(screen.getByRole("button", { name: "Check out" }));
    expect(await screen.findByText("Order 7 placed")).toBeInTheDocument();
    expect(placeOrder).toHaveBeenCalledTimes(1);
  });
});
```

Three lines moved. The imports come from one module. The module mock became a mock function behind the service the checkout action calls, so the real action runs under the spec's `ctx` and the contract checks what the mock answers. `render` is awaited.

## The lab

Take one spec from a suite you have and move it with the list above. Run `fsr test app <name>`, where the name narrows the run to that spec. The first failure is usually one of three: an unawaited `render` or event, a `vi.mock` with nowhere to go or a mock answering a shape the contract refuses, which the failure names by field.
