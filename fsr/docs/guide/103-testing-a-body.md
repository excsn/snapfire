# 103. Testing a body

The question this chapter answers: how do you test a loader or an action without a backend, without Node and without the test running somewhere the body does not?

**For:** app developers.

## A test replays the body where it runs

A body is data the interpreter runs, so a test of a body is a replay: build the context the body would see, run it, look at what came out. `fsr test` does exactly that. It lowers the test file, lowers the body under test the way the build does, then replays the body through the same interpreter that serves requests. Nothing in the path is a JavaScript engine, so the developer's test runs where the developer's code runs.

Tests live under `app/tests/`, mirroring `routes/`; they import the body by alias:

```ts
// app/tests/cart/loader.test.ts
import { load } from "@routes/cart/loader";
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

`ctx()` takes what the request would have carried: `session`, `services`, `input`, `params`, `query`, `identity`. A service method is a plain function of its arguments or a value when the arguments do not matter. The route in the type argument makes `params` type-check; the input type, `ctx<AddToCart>`, does the same for an action.

## The mock cannot lie

The mocked services sit behind the same registry the host uses, with the application's contract. A mock for a method the contract does not have fails with the method's name. A mock that answers a shape the contract rejects fails the same way, naming the field. That is what keeps a body test honest about the world it pretends to see: the first version of the storefront's checkout test returned order lines without a `name`; the test failed before the assertion was reached, which is the failure you want to have at your desk rather than in a page.

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

## Route tests are the other layer

A body test cuts at the service boundary. A route test cuts at the host: a request in, a document or payload out, with every transport mocked. The storefront's Rust suite is that layer, seventeen tests over a mock transport that assert on the HTML a route renders and the props it ships. They are Rust because they assert on the host; an application with no Rust project has nothing of its own to assert there. The two layers are enough: a body test says a loader produces these props from these responses; a route test says a URL produces this document from these props.

## The lab

Run `fsr test app`. Seven tests pass, each printed with its file and name. Now open `actions.test.ts` and change the mocked `placeOrder` to return `{ id: 7n }` only. Run again: the checkout test fails; the message names `shopping.placeOrder()` and the missing field, because the registry checked the mock's answer against the contract before the body ever saw it.

Put it back, then change the expected count in the first test to `5n`. The failure prints `actual: 4n` and `expected: 5n`, in the value model's own spelling.
