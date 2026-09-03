# 101. Actions and the session

The question this chapter answers: how does the browser change something, how is the input typed, where does the session live and what stops an empty cart from reaching the order service?

**For:** app developers.

## An action is a declared, typed mutation

An action is an exported constant in a route's `actions.ts`, built with `action` around a body whose parameter names its input type:

```ts
export const addToCart = action(async ({ input, session }: ActionCtx<AddToCart>) => {
  const key = String(input.product_id);
  const wanted = (session.cart[key] ?? 0n) + input.quantity;
  if (wanted <= 0n) delete session.cart[key];
  else session.cart = { ...session.cart, [key]: wanted };
  const count = Object.values(session.cart).reduce((n, q) => n + q, 0n);
  return { lines: session.cart, count };
});
```

`AddToCart` is an interface under `app/schemas/`; the build lowers every interface there into the contract beside the imported services. The host checks an action's input against its schema before the body runs, so a body never sees a shape it did not declare. The annotation on the parameter is what tells the build which schema. It is also what lets TypeScript infer the action's result for the browser and the tests; `action<AddToCart>(...)` reads too, but with an explicit type argument TypeScript stops inferring the rest, so the parameter form is the one to write.

The build declares every action in the plan file by id, `cart.addToCart`, so the host refuses to boot if any declared action has nothing answering it. An action a page can call that nobody implemented is a boot error, never a 404 in production.

## The session is a typed record

`app/schemas/session.ts` declares the session's shape as an interface, plus `defaults` for what a body reads when a key is absent:

```ts
export interface Session {
  cart: Record<string, bigint>;
}

export const defaults: Session = { cart: {} };
```

A body reads `session.cart` and gets `{}` on a fresh session rather than `undefined`, because the build folds the default into every read it lowers. Writes are statements the interpreter applies: `session.cart = ...` and `delete session.cart[key]`. They land in a draft and commit only when the body finishes; a body that fails halfway leaves the session untouched. The write itself is what the host persists, through the signed cookie and the store [chapter 203](203-sessions-and-identity.md) describes; a body never sees a cookie.

The typed shape is why the cart is written as `session.cart = { ...session.cart, [key]: wanted }` rather than an index assignment. With `cart` typed as a record, `session.cart[key] = wanted` is a type error when the record may be absent; the honest TypeScript is the spread with a computed key. The recogniser learned the computed key from that body.

## Guards run first

`fail(kind, message)` inside an `if` is a guard. The kinds are the seven the runtime maps onto a status: `unauthorized`, `not_found`, `invalid`, `conflict`, `timeout`, `unavailable`, `internal`. The storefront's checkout has one:

```ts
if (lines.length === 0) fail("invalid", "the cart is empty");
```

A guard that reads nothing a call has to produce runs before any call is made, so an empty cart never reaches the order service, which is an assertion the chapter 103 test states in so many words. A guard that depends on a call's result runs where it sits.

## Calling an action from the browser

The build writes one typed callable per action into `generated/client.ts`, nested by route: `actions.cart.addToCart({ product_id, quantity })` returns the body's result, typed. The client holds action ids, never URLs; the host answers them at one path and checks the input before dispatch. A failure comes back as an `ActionFailure` carrying the kind and the message the guard gave, which is what the storefront's toast shows.

A successful call re-fetches the current route by default and patches the segments that changed, so the header's badge follows the cart without a page reload and without the page asking. A call that should not revalidate says so when it is created.

## The lab

Run `fsr check app`, then remove the `export` from `checkout` in `actions.ts` and check again: the report's actions section loses `cart.checkout`, `generated/client.ts` loses its callable and `tsc` fails in the cart page at the call. A page cannot call an action the build did not declare. Put it back.

Then run `fsr test app checkout`. The test named "checkout refuses an empty cart before any call" asserts that the order service saw nothing, which is the guard running first; the one after it places the held lines and asserts the cart came back empty in the same body. Add a product in the browser and check out to watch the second one happen for real.
