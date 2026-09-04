# 002. A body is data

The question this chapter answers: why is a loader read rather than run, what happens to one the build cannot read and why does the report always say where a body runs?

**For:** everyone.

## What a loader actually says

Here is the storefront's cart loader, whole:

```ts
export async function load({ session, services }: Ctx<"/cart">) {
  const catalog = await services.shopping.listProducts({});
  const lines = catalog.filter((p) => session.cart[String(p.id)]).map((p) => ({ ...p, quantity: session.cart[String(p.id)] }));
  const cartCount = Object.values(session.cart).reduce((n, q) => n + q, 0n);
  return { lines, cartCount };
}
```

It reads the session, calls one service, filters, maps, reduces and returns an object. Nothing in it needs a JavaScript engine to be understood. It is a small program in a small language: reads from the request, literals, field access, arithmetic, comparisons, lambdas over arrays, service calls and a return. fsr calls that language the IR; the build's job is to recognise a body as a sentence in it.

The recognised body is written into the plan file as data. The interpreter in the host runs it against the request: it evaluates the reads, makes the calls, applies the lambdas and hands the page its props. Independent awaits run together, since the plan can see they do not depend on each other. A guard that reads nothing bound yet runs before any call, so an empty cart never reaches the order service. Session writes land in a draft that commits only when the body finishes, so a failed action leaves the session as it found it.

**The interpreter does not run your TypeScript. It runs what your TypeScript meant.** That is what makes the same body cost the same whether it stays in TypeScript or is taken over in Rust: both are the rows.

## Residue

A body that says something outside the language is residue. `try`, a class, a regular expression, a `while`, an import the build cannot follow, a call it does not know. The build does not guess. It names the file, the line and the construct:

```
app/routes/cart/page.loader.ts:2:9: `try`
```

and stops, because today there is no engine to hand the body to. That is a deliberate floor rather than a gap. The failure fsr is built to prevent is the silent one, where a build quietly moves a body from "data the runtime executes" to "JavaScript some engine runs" so that an application that meant to have no server JavaScript grows one without anybody deciding. When an engine exists, residue will run there and the report will say `engine` beside the name. It will never say nothing.

The language grows when an application shows a body that needs a construct and cannot be written another way. The storefront's cart forced computed keys, its catalog forced the query string, its pages forced `Math.round` and `toFixed`. Each arrived with the body that needed it; each is one more thing every fsr application can now say.

## The report

Every name the application declares appears in the report with who answers it:

```
sources   index                  lowered     routes/index/page.loader.ts
          cart                   lowered     routes/cart/page.loader.ts
actions   cart.addToCart         lowered     routes/cart/actions.ts
```

The build prints it after lowering and the host prints it at boot, from the same data, so what a developer saw is what runs. `lowered` means the body is IR and the interpreter runs it. `rust` means a Rust function answers the name and the plan file only declares it. `rust override` means the plan file lowered a body and Rust took the name back deliberately, which [chapter 201](201-graduating-to-rust.md) covers with the rule that makes the override explicit.

The report is the whole of the placement story. There is no configuration that moves a body somewhere else, no annotation that changes where it runs. A body runs where the report says; the report is derived from the artifacts.

## Why data and not a runtime

Three things follow from a body being data; each is worth more than a faster loader.

It can be inspected. The plan file is JSON and a body's rows can be read, diffed in a pull request and reasoned about without running anything.

It can be tested where it runs. [Chapter 103](103-testing-a-body.md) replays a body through the same interpreter against a mocked context, so a test never runs somewhere the code does not.

It can be replaced piecewise. A Rust function that answers the same name is the same rows from the caller's side, which is why a platform developer can graduate one hot loader without the page, the tests or the other loaders noticing.

## The lab

Wrap the cart loader's service call in `try { ... } catch { ... }` and run `fsr check app`. The build stops at the line with the word `try`. Remove it and the report shows `cart` as `lowered` again.

Then break a call instead: change `listProducts` to `listProduct` and check again. This time the contract refuses the method by name before anything is lowered, since a call to a method the contract does not have is not a body the runtime could ever answer.
