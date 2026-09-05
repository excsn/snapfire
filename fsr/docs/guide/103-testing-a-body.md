# 103. Testing a body and a page

The question this chapter answers: how do you test a loader, an action or a page without a backend, without Node and without the test running somewhere the code does not?

**For:** app developers.

## A test replays the body where it runs

A body is data the interpreter runs, so a test of a body is a replay: build the context the body would see, run it, look at what came out. `fsr test` does exactly that. It lowers the test file, lowers the body under test the way the build does, then replays the body through the same interpreter that serves requests. Nothing in the path is a JavaScript engine, so the developer's test runs where the developer's code runs.

Tests live under `app/tests/`, mirroring `routes/`; they import the body by alias:

```ts
// app/tests/cart/loader.test.ts
import { load } from "@routes/cart/page.loader";
import { assert, ctx, test } from "@snapfire/fsr/testing";

test("held lines carry the catalog's rows and the held quantity", async () => {
  const c = ctx<void, "/cart">({
    session: { cart: { "2": 3n } },
    services: { shopping: { listProducts: () => [filament, hotend] } },
  });
  const { lines, cartCount } = await load(c);
  assert.equal(lines, [{ ...hotend, quantity: 3n }]);
  assert.equal(cartCount, 3n);
});
```

`ctx()` takes what the request would have carried: `session`, `services`, `input`, `params`, `query`, `identity`, `locale`. A service method is a plain function of its arguments or a value when the arguments do not matter. The route in the type argument makes `params` type-check; the input type, `ctx<AddToCart>`, does the same for an action.

## The mock cannot lie

The mocked services sit behind the same registry the host uses, with the application's contract. A mock for a method the contract does not have fails with the method's name. A mock that answers a shape the contract rejects fails the same way, naming the field. A call to a method no mock answers fails too, naming the method: under a page spec the loader would otherwise degrade to the error component and the spec would pass by looking at the wrong page. That is what keeps a body test honest about the world it pretends to see: the first version of the storefront's checkout test returned order lines without a `name`; the test failed before the assertion was reached, which is the failure you want to have at your desk rather than in a page.

## What a test can say

The file is a small dialect and the runner refuses anything outside it with the line, so a test never silently does less than it reads. It holds imports, `const` fixtures the tests share and `test` blocks. Inside a block:

- `const c = ctx({...})`, the context, bound to a name.
- `await load(c)`, `const result = await addToCart(c)`, `const { lines } = await load(c)`: runs of the loader or an action, bound or not.
- `assert.ok(x)`, `assert.equal(actual, expected)`, `await assert.rejects(checkout(c), "invalid")`: the three assertions. `equal` is deep and compares the way the value model does, so `1n` and `1` are different and a failed comparison prints both sides as TypeScript would write them.

After every run the context refreshes: `c.session` is the session as the body left it, `c.trace.calls` is every service call it made with its arguments and `c.trace.session.written` names the keys it wrote. Those are the assertions that say what a body did rather than only what it returned:

```ts
await assert.rejects(checkout(c), "invalid");
assert.equal(c.trace.calls, []);
```

That test is the sentence "an empty cart never reaches the order service" made checkable.

## A page test renders where the browser would

A body test never touches a component. A page test does: it renders a page or a component into a DOM, clicks it, reads what it shows. `fsr test` runs those too, in QuickJS inside the same process, over a DOM from linkedom and React's own development build, so the page runs as JavaScript because a page is JavaScript. No Node is involved: snapfirec compiles the spec file beside the app's modules into `app/.fsr-test/`, the engine resolves imports through the app's import map and the vendor tree, and the few test-only builds it needs are fetched once into the same directory.

A page test is a `*.spec.tsx` under `app/tests/`, and its surface reads like the testing library you already know:

```tsx
// app/tests/product/page.spec.tsx
import ProductPage from "@routes/product/[id]/page";
import { advance, assert, ctx, fireEvent, render, screen, test } from "@snapfire/fsr-client/testing";

test("choosing a quantity and adding runs the action with it", async () => {
  const c = ctx({ session: { cart: {} } });
  await render(<ProductPage product={product} stock={stock} inCart={0n} cartCount={0n} />, { ctx: c });
  await fireEvent.change(screen.getByLabelText("Quantity"), "3");
  await fireEvent.click(screen.getByText("Add to cart"));
  assert.equal(c.session.cart, { "1": 3 });
  assert.ok(screen.getByText("Added to your cart"));
  await advance(5000);
  assert.equal(screen.queryByText("Added to your cart"), null);
});
```

Three things in that test are not what a jest test would do.

**The page is hydrated, not mounted.** `render` of a page the build lowered first asks the server renderer for the page's HTML with those props, puts it in the container and lets React hydrate over it, exactly the production sequence. A mismatch between what Rust rendered and what React expected fails the test with React's own message, naming the element and both sides, because the development build says it in words rather than as an error number. The first version of this runner failed the cart page with `Prop style did not match. Server: "" Client: "null"`, which was a real difference the browser had been patching silently. `r.hydrated` names the module that was hydrated, or is `null` when the component mounted fresh, which is what happens to a component below a page.

**The action is real.** The click calls `addToCart` through the generated client, which posts to `/_sf/action/cart.addToCart`. Under test that `fetch` is answered by the runner: the lowered action runs through the interpreter under the `ctx` the test built, the session and the trace update the way they do in a body test, and the page gets the same JSON it would from the host. A mocked service method is a function in the spec; when the action calls it, the interpreter calls back into the page's JavaScript and the contract checks both the arguments and the answer, so a mock cannot lie here either.

**Time does not pass.** `settle`, which `render` and every `fireEvent` await, runs everything that happens now: microtasks, the action round trip, React's re-render, timers already due. A timer set for later waits for `advance(ms)`, so the toast is there to assert on and gone after the clock moves. Nothing in a test sleeps.

The queries are `screen.getByText`, `queryByText`, `getAllByText`, `getByLabelText`, `getByPlaceholderText` and `getByTestId`, each taking an optional root to search under, and the events are `fireEvent.click`, `change`, `submit` and `keyDown`. A `console.error` during a test fails it with the text, since React reports what it does not like that way. A failing test prints everything the page logged.

**A route loads the way a browser loads it.** `load("/", { ctx })` fetches the document the stock host renders for that route, with the spec's mocks behind every service, installs it as the document, mounts its islands and enables navigation. A click on a link is then a client navigation: the navigator fetches the destination's payload from the same host, swaps the segment whose key changed and hydrates the new page in place. The storefront's [`navigation.spec.tsx`](../../examples/shopping_react_ts/app/tests/navigation.spec.tsx) clicks from the catalog to the cart and asserts that `#app` is the same element afterwards and that each page's loader ran once through the mocks. [`layout.spec.tsx`](../../examples/shopping_react_ts/app/tests/layout.spec.tsx) types into the header, navigates, adds to the cart and asserts the header kept both its DOM and its text while its count changed. A page's own `navigate` or `refresh` after an action reaches the same host, so a spec sees the loader run again. The host under test is built from `config/app.toml` beside the app, the one that serves; without it, a route fetch answers 404 and says so. `load` follows a redirect the middleware answers and returns the status and the path it landed on, so a spec can load a path nothing matches and assert both the `404` and the not-found page the host rendered for it. It can also load an old path and assert where it went.

```tsx
test("a click from the catalog to the cart swaps the page and keeps the document", async () => {
  const c = ctx({ session: { cart: { "1": 2n } }, services: { shopping: { listProducts: () => [filament] } } });
  await load("/", { ctx: c });
  const app = document.getElementById("app");
  await fireEvent.click(screen.getByLabelText("Cart, 2 items"));
  assert.equal(location.pathname, "/cart");
  assert.ok(document.getElementById("app") === app);
  assert.ok(screen.getByText("Shopping cart"));
});
```

A route with a `loading.tsx` reaches a browser as a stream, and `load` reads it whole: each resolved template is moved into its slot before the islands mount, and what the fill script would have said about the title and the store is applied once the document's own seed is in, so a spec sees the resolved page, the retitled document and the seeded keys, never the skeleton. The store is emptied before every `load`, the way a full load empties it in a browser, so a key one test wrote cannot leak into the next test's hydration. What `load` does not do is run the application's entry module: `src/main.ts` and whatever it wires, a `derive`, a listener, a global, are the browser's, and a spec sees a derived key only as the server seeded it. A mock returns what the contract says, and one thing it cannot spell: an integral value for a `number` field, since `0` encodes as an integer and the contract refuses an integer where a double is declared, so a mock writes `0.5` where the backend would write `0.0`.

## Route tests are the other layer

A body test cuts at the service boundary. A route test cuts at the host: a request in, a document or payload out, with every transport mocked. The storefront's Rust suite is that layer, nineteen tests over a mock transport that assert on the HTML a route renders, the chunks a deferred route streams and the props it ships. They are Rust because they assert on the host; an application with no Rust project gets the document half of that from `load` in a spec. The two layers are enough: a body test says a loader produces these props from these responses; a route test says a URL produces this document from these props.

## The lab

Run `fsr test app`. Thirty tests pass, seven body tests and twenty-three specs, each printed with its file and name. Now open `actions.test.ts` and change the mocked `placeOrder` to return `{ id: 7n }` only. Run again: the checkout test fails; the message names `shopping.placeOrder()` and the missing field, because the registry checked the mock's answer against the contract before the body ever saw it.

Put it back, then change the expected count in the first test to `5n`. The failure prints `actual: 4n` and `expected: 5n`, in the value model's own spelling.

Now the page tests. Open [`page.spec.tsx`](../../examples/shopping_react_ts/app/tests/cart/page.spec.tsx) under `tests/cart` and change the fixture's `image` to a string. Run again: the runner reports that rendering the page failed, since the server renderer refuses a `background` that is not there, and `tsc` says the same thing in its own words. Put it back and remove `name` from the lines the mocked `placeOrder` answers in the checkout test: the contract rejects the mock's answer before the page sees it, from inside a click.
