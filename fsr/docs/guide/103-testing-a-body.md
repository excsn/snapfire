# 103. Testing a body and a page

The question this chapter answers: how do you test a loader, an action or a page without a backend, without Node and without the test running somewhere the code does not?

**For:** app developers.

The testing API takes its names from Jest, Vitest and Testing Library: `describe`, `it`, `expect`, `beforeEach`, `fn`, `screen.getByRole` and `userEvent` read the way they do there, so a suite written for those moves over with small changes. [107](107-moving-tests-from-jest-or-vitest.md) lists them.

## A test replays the body where it runs

A body is data the interpreter runs, so a test of a body is a replay: build the context the body would see, run it, look at what came out. `fsr test` does exactly that. It lowers the test file, lowers the body under test the way the build does, then replays the body through the same interpreter that serves requests. Nothing in the path is a JavaScript engine, so the developer's test runs where the developer's code runs.

Tests live under `app/tests/`, mirroring `routes/`; they import the body by alias:

```ts
// app/tests/cart/loader.test.ts
import { load } from "@routes/cart/page.loader";
import { ctx, describe, expect, it } from "@snapfire/fsr/testing";

describe("the cart loader", () => {
  it("carries the catalog's rows and the held quantity", async () => {
    const c = ctx<void, "/cart">({
      session: { cart: { "2": 3n } },
      services: { shopping: { listProducts: () => [filament, hotend] } },
    });
    const { lines, cartCount } = await load(c);
    expect(lines).toEqual([{ ...hotend, quantity: 3n }]);
    expect(cartCount).toEqual(3n);
  });
});
```

`ctx()` takes what the request would have carried: `session`, `services`, `input`, `params`, `query`, `identity`, `locale`. A service method is a plain function of its arguments, a value when the arguments do not matter or a mock function. The route in the type argument makes `params` type-check; the input type, `ctx<AddToCart>`, does the same for an action.

## The mock cannot lie

The mocked services sit behind the same registry the host uses, with the application's contract. A mock for a method the contract does not have fails with the method's name. A mock that answers a shape the contract rejects fails the same way, naming the field. A call to a method no mock answers fails too, naming the method: under a page spec the loader would otherwise degrade to the error component and the spec would pass by looking at the wrong page. That is what keeps a body test honest about the world it pretends to see: the first version of the storefront's checkout test returned order lines without a `name`; the test failed before the assertion was reached, which is the failure you want to have at your desk rather than in a page.

The same allowance holds on the way in. A mock answering `minutes: 35` for an integer field is read as the integer the contract names, since a JavaScript number is a double whatever it holds; a mock answering `35.5` there still fails with the field. The generated mock types agree, taking `number` wherever a field is `bigint`.

## What a test can say

The file is a small dialect and the runner refuses anything outside it with the line, so a test never silently does less than it reads. At the top of the file or of a `describe` it holds imports, `const` fixtures, mock functions, `let` bindings a hook assigns, hooks, `describe` blocks and tests:

- `describe(name, () => { ... })` groups tests. A test's name in the report is the names of the blocks around it joined to its own with ` > `. `it` and `test` are the same function.
- `test.skip`, `test.only`, `test.todo(name)`, `describe.skip`, `describe.only` and the `xit`, `fit`, `xdescribe` and `fdescribe` spellings mark what runs. An `only` anywhere in the file skips every test it does not cover. The report counts skipped and todo tests beside passed and failed ones.
- ``test.each([[1n, 2n], [3n, 4n]])("%s and %s", async (a, b) => { ... })`` runs the body once per row, an array row spread over its parameters. A template table, ``test.each`word | length ${"leek"} | ${4}` ``, passes each row as one object. The name takes printf placeholders for the arguments, `%#` for the row's index and `$field` for a field of an object row. `describe.each` repeats a whole block the same way.
- `beforeAll`, `afterAll`, `beforeEach` and `afterEach` hold what a test holds. A hook assigns a `let` the file or the block declared, `c = ctx({ ... })`, which is how one context reaches every test under it. What a `beforeAll` builds is shared by those tests: a session it created carries what the first test wrote into the second. `afterEach` runs after a failure too.

Inside a test or a hook:

- `const c = ctx({...})`, the context, bound to a name.
- `await load(c)`, `const result = await addToCart(c)`, `const { lines } = await load(c)`: runs of the loader or an action, bound or not.
- `const head = meta({ data })`, `const seeded = store({ data })`: the loader module's other two exports over data, usually the data the run above it returned. Both are lowered and replayed like everything else, so a title built from what a loader found is checkable without rendering the page, as are the store keys a route seeds. Each runs against the `ctx` bound above it, since either may read the locale or the identity.
- `const expected = { ... }`: a local value.
- `expect(actual).toEqual(expected)` and the other matchers: `toBe`, `toStrictEqual`, `toBeTruthy`, `toBeFalsy`, `toBeNull`, `toBeUndefined`, `toBeDefined`, `toBeNaN`, the four comparisons, `toBeCloseTo`, `toContain`, `toContainEqual`, `toHaveLength`, `toHaveProperty`, `toMatch`, `toMatchObject`, `toBeTypeOf` and `toBeOneOf`, each under `.not` as well. `expect(value, "message")` puts the message first in a failure's report. Equality compares the way the value model does, with one allowance: an integer field reads back as a bigint while a test may write the number it stands for, so `35` and `35n` are the same value where either side is whole. `toStrictEqual` drops the allowance. `toBe` compares the way `toEqual` does, since a value the interpreter returns has no identity of its own.
- `expect.any(String)`, `expect.anything()`, `expect.objectContaining({ ... })`, `expect.arrayContaining([...])`, `expect.stringContaining("x")` and `expect.stringMatching(/x/)` sit anywhere inside an expected value.
- `await expect(checkout(c)).rejects.toMatchObject({ kind: "invalid" })` asserts a run fails with that kind. `.rejects.toThrow("invalid")` is looser: the kind or the message must hold the text. `.resolves` matches what a run returned.
- `const placeOrder = fn((args) => order)` is a mock function, which a ctx's services name: `services: { shopping: { placeOrder } }`. `expect(placeOrder).toHaveBeenCalledWith({ lines })`, `toHaveBeenCalledTimes(1)` and the other call matchers read what it was asked. `placeOrder.mockReturnValueOnce(other)`, `mockResolvedValue`, `mockImplementation` and `mockRejectedValue({ kind: "unavailable", message: "down" })` change what it answers, the last one failing the call with that kind. A mock function declared at the top of a file starts every test with no calls.
- `assert.ok`, `assert.equal`, `assert.match` and `assert.rejects` still read, for tests written before `expect`.

A failed comparison prints both sides as TypeScript would write them, under the message when there is one.

After every run the context refreshes: `c.session` is the session as the body left it, `c.trace.calls` is every service call it made with its arguments and `c.trace.session.written` names the keys it wrote. Those are the expectations that say what a body did rather than only what it returned:

```ts
await expect(checkout(c)).rejects.toMatchObject({ kind: "invalid" });
expect(c.trace.calls).toEqual([]);
```

That test is the sentence "an empty cart never reaches the order service" made checkable.

## A page test renders where the browser would

A body test never touches a component. A page test does: it renders a page or a component into a DOM, clicks it, reads what it shows. `fsr test` runs those too, in QuickJS inside the same process, over a DOM from linkedom and React's own development build, so the page runs as JavaScript because a page is JavaScript. No Node is involved: snapfirec compiles the spec file beside the app's modules into `app/.fsr-test/`, the engine resolves imports through the app's import map and the vendor tree and the few test-only builds it needs are fetched once into the same directory.

A page test is a `*.spec.tsx` under `app/tests/`:

```tsx
// app/tests/product/page.spec.tsx
import ProductPage from "@routes/product/[id]/page";
import { advance, ctx, expect, render, screen, test, userEvent } from "@snapfire/fsr-client/testing";

test("choosing a quantity and adding runs the action with it", async () => {
  const c = ctx({ session: { cart: {} } });
  await render(<ProductPage product={product} stock={stock} inCart={0n} cartCount={0n} />, { ctx: c });
  const user = userEvent.setup();
  await user.selectOptions(screen.getByLabelText("Quantity"), "3");
  await user.click(screen.getByRole("button", { name: "Add to cart" }));
  expect(c.session.cart).toEqual({ "1": 3 });
  expect(screen.getByText("Added to your cart")).toBeInTheDocument();
  await advance(5000);
  expect(screen.queryByText("Added to your cart")).toBeNull();
});
```

Four things in that test differ from the same test under a Node runner.

**The page is hydrated, not mounted.** `render` of a page the build lowered first asks the server renderer for the page's HTML with those props, puts it in the container and lets React hydrate over it, exactly the production sequence. A mismatch between what Rust rendered and what React expected fails the test with React's own message, naming the element and both sides, because the development build says it in words rather than as an error number. The first version of this runner failed the cart page with `Prop style did not match. Server: "" Client: "null"`, which was a real difference the browser had been patching silently. `r.hydrated` names the module that was hydrated or is `null` when the component mounted fresh, which is what happens to a component below a page and to a page with nothing for the browser to run.

**The action is real.** The click calls `addToCart` through the generated client, which posts to `/_sf/action/cart.addToCart`. Under test that `fetch` is answered by the runner: the lowered action runs through the interpreter under the `ctx` the test built, the session and the trace update the way they do in a body test and the page gets the same JSON it would from the host. A mocked service method is a function in the spec; when the action calls it, the interpreter calls back into the page's JavaScript and the contract checks both the arguments and the answer, so a mock cannot lie here either.

**Time does not pass.** `settle`, which `render`, every `fireEvent` and every `userEvent` method await, runs everything that happens now: microtasks, the action round trip, React's re-render, timers already due. A timer set for later waits for `advance(ms)`, so the toast is there to assert on and gone after the clock moves. `waitFor` and every `findBy` query retry after moving the clock in small steps, so something a timer brings in arrives while they wait. Nothing in a test sleeps.

**Rendering and acting are awaited.** `render` and every event return a promise, because each settles the engine before the next line reads the page.

`screen` holds the queries: `getByRole`, `getByText`, `getByLabelText`, `getByPlaceholderText`, `getByAltText`, `getByTitle`, `getByDisplayValue` and `getByTestId`, each with its `getAllBy`, `queryBy`, `queryAllBy`, `findBy` and `findAllBy` forms. `within(element)` holds the same over one element. A role is the element's ARIA role, written or implied by its tag. `{ name }` matches its accessible name, so `getByRole("button", { name: "Add to cart" })` finds the button a screen reader would announce that way. A role query that finds nothing lists the roles that were there. The matchers for markup read the page the same way: `toBeInTheDocument`, `toHaveTextContent`, `toBeVisible`, `toBeDisabled`, `toHaveValue`, `toBeChecked`, `toHaveFocus`, `toHaveAccessibleName` and the rest. `userEvent` types key by key, moves focus and clicks with a browser's default actions: a checkbox toggles, a label clicks its control, a submit button submits its form and Enter in a field submits it. `fireEvent` dispatches one event. `fn`, `spyOn` and `vi.fn` give mock functions, which a spec's `ctx` services may name as well. A `console.error` during a test fails it with the text, since React reports what it does not like that way. A failing test prints everything the page logged.

The runner lays nothing out, so visibility and the accessibility tree read attributes and inline styles, never a stylesheet. Focus, an input's typed value and a text control's selection are tracked the way a browser tracks them.

**A route loads the way a browser loads it.** `load("/", { ctx })` fetches the document the stock host renders for that route, with the spec's mocks behind every service, installs it as the document, mounts its islands and enables navigation. A click on a link is then a client navigation: the navigator fetches the destination's payload from the same host, swaps the segment that rendered something different and hydrates the new page in place. The storefront's [`navigation.spec.tsx`](../../examples/shopping_react_ts/app/tests/navigation.spec.tsx) clicks from the catalog to the cart and expects `#app` to be the same element afterwards and each page's loader to have run once through the mocks. [`layout.spec.tsx`](../../examples/shopping_react_ts/app/tests/layout.spec.tsx) types into the header, navigates, adds to the cart and expects the header to have kept both its DOM and its text while its count changed. A page's own `navigate` or `refresh` after an action reaches the same host, so a spec sees the loader run again. The host under test is built from `config/app.toml` beside the app, the one that serves; without it, a route fetch answers 404 and says so. `load` follows a redirect the middleware answers and returns the status and the path it landed on, so a spec can load a path nothing matches and expect both the `404` and the not-found page the host rendered for it. It can also load an old path and expect where it went.

```tsx
test("a click from the catalog to the cart swaps the page and keeps the document", async () => {
  const c = ctx({ session: { cart: { "1": 2n } }, services: { shopping: { listProducts: () => [filament] } } });
  await load("/", { ctx: c });
  const app = document.getElementById("app");
  await userEvent.click(screen.getByLabelText("Cart, 2 items"));
  expect(location.pathname).toEqual("/cart");
  expect(document.getElementById("app")).toBe(app);
  expect(screen.getByText("Shopping cart")).toBeInTheDocument();
});
```

What hydrates in a page test is what would hydrate in a browser. `render` of a lowered page hands React the values and subtrees the server computed for it, the way the mounter does from the island's props, so a spec exercises the read path rather than the fallback and a component the server rendered as a subtree is not rendered by React in the test either. An island in server mode is stepped the same way it is served: a click in a spec posts to the island route, the runner answers it through the function the host's route calls and the spec sees the patched markup, as [`order/server.spec.tsx`](../../examples/shopping_react_ts/app/tests/order/server.spec.tsx) does when it clicks the order help twice.

A route with a `loading.tsx` reaches a browser as a stream and `load` reads it whole: each resolved template is moved into its slot before the islands mount and what the fill script would have said about the title and the store is applied once the document's own seed is in, so a spec sees the resolved page, the retitled document and the seeded keys, never the skeleton. The store is emptied before every `load`, the way a full load empties it in a browser, so a key one test wrote cannot leak into the next test's hydration. What `load` does not do is run the application's entry module: `src/main.ts` and whatever it wires, a `derive`, a listener, a global, are the browser's. A spec sees a derived key only as the server seeded it. A mock returns what the contract says and one thing it cannot spell: an integral value for a `number` field, since `0` encodes as an integer and the contract refuses an integer where a double is declared, so a mock writes `0.5` where the backend would write `0.0`.

A Vue island mounts under the harness the way it does in a browser: the recipes example's specs click a Vue button, plan a recipe and read the count the Vue masthead shows, with the same `load`, `userEvent` and `screen`. The markup a layout writes inside a Vue island reaches it as its default slot. The runner compiles the browser half of the app the way the bundle does, static templates left out and `.vue` files through the plugin. It writes the build's generated files first, so a spec always runs against the registry of the build it was given.

## Route tests are the other layer

A body test cuts at the service boundary. A route test cuts at the host: a request in, a document or payload out, with every transport mocked. The storefront's Rust suite is that layer, nineteen tests over a mock transport that assert on the HTML a route renders, the chunks a deferred route streams and the props it ships. They are Rust because they assert on the host; an application with no Rust project gets the document half of that from `load` in a spec. The two layers are enough: a body test says a loader produces these props from these responses; a route test says a URL produces this document from these props.

## The lab

Run `fsr test app`. Every body test and spec prints with its file and name and the last line counts what passed, failed, was skipped and is still todo. Now open `tests/cart/actions.test.ts` and change the mocked `placeOrder` to return `{ id: 7n }` only. Run again: the checkout test fails; the message names `shopping.placeOrder()` and the missing field, because the registry checked the mock's answer against the contract before the body ever saw it.

Put it back, then change the expected count in the first test to `5n`. The failure prints `Expected: 5n` and `Received: 4n`, in the value model's own spelling. Add a message with `expect(result.count, "the count the session holds").toEqual(5n)` and run again: the message comes first.

Now the page tests. Open [`page.spec.tsx`](../../examples/shopping_react_ts/app/tests/cart/page.spec.tsx) under `tests/cart` and change the fixture's `image` to a string. Run again: the runner reports that rendering the page failed, since the server renderer refuses a `background` that is not there and `tsc` says the same thing in its own words. Put it back and remove `name` from the lines the mocked `placeOrder` answers in the checkout test: the contract rejects the mock's answer before the page sees it, from inside a click.
