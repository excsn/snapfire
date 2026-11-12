# 901. JavaScript's built-ins

Every member of JavaScript's standard library, with what fsr does when a loader or a component reads it. The list at the end is written from the table the build is tested against, so it changes when the build does.

**For:** app developers checking whether a built-in works before writing it. Also for anyone reading a refusal.

## How to read the list

Each member has two answers, one for a loader or an action and one for a component.

- **server**: the build lowers it and the server answers what node v26 answers for the same code. The test runs the member's code through node and through fsr and compares the two.
- **browser**: the value depends on the viewer's machine or the moment the page runs, such as `Date.now()` or `Math.random()`. The server leaves it to the browser, as [chapter 111](111-values-the-server-and-browser-disagree-on.md) describes. Only a component has this answer, since a loader always runs on the server.
- **refused**: the build stops with the file, the line and what it could not lower. Nothing runs that would answer differently from node.
- **throws, as in node**: node throws for this code and so does fsr.
- **loader only**: the code awaits, which a component cannot do.

The code each member was tried with is in `fsr/lower/tests/standard_members.tsv`, one line per member with node's answer beside it.

## A refusal

A refusal names the line and the member:

```ts
async function fetchTotal() {
  return 21;
}

export async function load() {
  const total = fetchTotal().then((n) => n * 2);
  return { total };
}
```

```
routes/page.loader.ts:6:17: `.then()`, which is not a builtin
```

Write the `await` instead. The build lowers it:

```ts
export async function load() {
  const total = (await fetchTotal()) * 2;
  return { total };
}
```

## What is refused and what to write instead

| Refused | Write instead |
| --- | --- |
| `.then()`, `.catch()` and `.finally()` on a promise | `await`, with `try` and `catch` around it |
| `Object.defineProperty`, `Object.defineProperties`, `Object.setPrototypeOf`, their `Reflect` forms and the `__defineGetter__` family | A plain object literal or a class with a getter |
| `console.log(...)` used as a value | `console.log(...)` as a statement of its own |
| `RegExp.$1`, `RegExp.lastMatch` and the other legacy statics | The match array `exec` or `match` answers |
| `Atomics` and `SharedArrayBuffer.prototype.grow` | Nothing: a body runs on one thread |
| `Error.captureStackTrace` and the other V8-only statics | A `new Error(message)` |
| An `Intl` formatter or a constructor kept as a value, `const f = Intl.NumberFormat` | `new Intl.NumberFormat(locale, options)` and its `format` called where it is used |
| `new Headers(...)` and `AbortController` | `headers` in a `Response` init, read back with `response.headers.get(name)` |
| `formatRange`, `formatRangeToParts` and `selectRange` | Two `format` calls joined |
| `formatToParts` of an `Intl.DurationFormat` | `format` |
| `Intl.Locale.prototype.getCollations` | Nothing: the collations are ICU's and the build does not carry them |
| A `Request`'s `body`, `headers`, `signal` and body methods | The fields the init gave, read back from the request |
| A `Response`'s body read without `await`, its `body` stream, `clone()` and `headers` as a value | `await response.text()`, `await response.json()` or `await response.bytes()` |
| The value `.append()`, `.set()`, `.delete()` or `.sort()` of a `URLSearchParams` answers | The call as a statement of its own, then the params read |
| `ArrayBuffer.prototype.resize` and `transfer` | A new `ArrayBuffer` of the size wanted |
| `TextEncoder.prototype.encodeInto` | `encode`, which answers a new `Uint8Array` |
| `Proxy.revocable` | `new Proxy(target, handler)` |
| `URL.createObjectURL` and `revokeObjectURL` | Nothing on the server: they name a `Blob` the browser holds |

## Where fsr answers as Chrome does

`Math.sin`, `Math.cbrt` and the other transcendental functions answer the correctly rounded result, as Chrome 158 does. Node v26 runs an older V8 whose results are one unit in the last place off for some arguments. The list marks the two members the test caught.

fsr's locale data is CLDR 48.2.1 and node v26.8.1 ships CLDR 48.0. Where the two differ fsr follows its own data. `new Intl.Locale("az-Arab").getNumberingSystems()` answers `arabext` in fsr and `latn` in node.

## Known limits

- `subarray` of a typed array is a copy, so a write through it does not reach the array it came from.
- A `Date` whose kind the build cannot see on a render path is written in UTC.
- `Temporal.Instant.prototype.toLocaleString` with no `timeZone` on a render path is written in the server's zone.
- A `Response` body can be read twice. JavaScript throws the second time.
- `new Request(path)` with a path rather than a URL is kept as given, so a middleware's `next` can run it. Node throws for it.
- `getTimeZones` of a locale whose region is a UN area code, such as `en-001`, answers an empty list.
- `resolvedOptions` of an `Intl.DurationFormat` given a unit's own style is refused.

## The list

<!-- fsr:members -->

### Object

| Member | Loader | Component |
| --- | --- | --- |
| `assign` | server | server |
| `getOwnPropertyDescriptor` | server | server |
| `getOwnPropertyDescriptors` | server | server |
| `getOwnPropertyNames` | server | server |
| `getOwnPropertySymbols` | server | server |
| `hasOwn` | server | server |
| `is` | server | server |
| `preventExtensions` | server | server |
| `seal` | server | server |
| `create` | server | server |
| `defineProperties` | refused | refused |
| `defineProperty` | refused | refused |
| `freeze` | server | server |
| `getPrototypeOf` | server | server |
| `setPrototypeOf` | refused | refused |
| `isExtensible` | server | server |
| `isFrozen` | server | server |
| `isSealed` | server | server |
| `keys` | server | server |
| `entries` | server | server |
| `fromEntries` | server | server |
| `values` | server | server |
| `groupBy` | server | server |
| `__defineGetter__` | refused | refused |
| `__defineSetter__` | refused | refused |
| `hasOwnProperty` | server | server |
| `__lookupGetter__` | refused | refused |
| `__lookupSetter__` | refused | refused |
| `isPrototypeOf` | refused | refused |
| `propertyIsEnumerable` | server | server |
| `toString` | server | server |
| `valueOf` | server | server |
| `__proto__` | server | server |
| `toLocaleString` | server | server |

### Function

| Member | Loader | Component |
| --- | --- | --- |
| `length` | server | server |
| `name` | server | server |
| `apply` | server | refused |
| `bind` | server | refused |
| `call` | server | refused |
| `toString` | server | server |
| `arguments` | throws, as in node | throws, as in node |
| `caller` | throws, as in node | throws, as in node |

### Array

| Member | Loader | Component |
| --- | --- | --- |
| `isArray` | server | server |
| `from` | server | server |
| `fromAsync` | server | loader only |
| `of` | server | server |
| `length` | server | server |
| `at` | server | server |
| `concat` | server | server |
| `copyWithin` | server | server |
| `fill` | server | server |
| `find` | server | server |
| `findIndex` | server | server |
| `findLast` | server | server |
| `findLastIndex` | server | server |
| `lastIndexOf` | server | server |
| `pop` | server | server |
| `push` | server | server |
| `reverse` | server | server |
| `shift` | server | server |
| `unshift` | server | server |
| `slice` | server | server |
| `sort` | server | server |
| `splice` | server | server |
| `includes` | server | server |
| `indexOf` | server | server |
| `join` | server | server |
| `keys` | server | server |
| `entries` | server | server |
| `values` | server | server |
| `forEach` | server | server |
| `filter` | server | server |
| `flat` | server | server |
| `flatMap` | server | server |
| `map` | server | server |
| `every` | server | server |
| `some` | server | server |
| `reduce` | server | server |
| `reduceRight` | server | server |
| `toReversed` | server | server |
| `toSorted` | server | server |
| `toSpliced` | server | server |
| `with` | server | server |
| `toLocaleString` | server | server |
| `toString` | server | server |

### Number

| Member | Loader | Component |
| --- | --- | --- |
| `isFinite` | server | server |
| `isInteger` | server | server |
| `isNaN` | server | server |
| `isSafeInteger` | server | server |
| `parseFloat` | server | server |
| `parseInt` | server | server |
| `MAX_VALUE` | server | server |
| `MIN_VALUE` | server | server |
| `NaN` | server | server |
| `NEGATIVE_INFINITY` | server | server |
| `POSITIVE_INFINITY` | server | server |
| `MAX_SAFE_INTEGER` | server | server |
| `MIN_SAFE_INTEGER` | server | server |
| `EPSILON` | server | server |
| `toExponential` | server | server |
| `toFixed` | server | server |
| `toPrecision` | server | server |
| `toString` | server | server |
| `valueOf` | server | server |
| `toLocaleString` | server | server |

### Boolean

| Member | Loader | Component |
| --- | --- | --- |
| `toString` | server | server |
| `valueOf` | server | server |

### String

| Member | Loader | Component |
| --- | --- | --- |
| `fromCharCode` | server | server |
| `fromCodePoint` | server | server |
| `raw` | server | server |

### Symbol

| Member | Loader | Component |
| --- | --- | --- |
| `for` | server | server |
| `keyFor` | server | server |
| `asyncIterator` | server | server |
| `hasInstance` | server | server |
| `isConcatSpreadable` | server | server |
| `iterator` | server | server |
| `match` | server | server |
| `matchAll` | server | server |
| `replace` | server | server |
| `search` | server | server |
| `species` | server | server |
| `split` | server | server |
| `toPrimitive` | server | server |
| `toStringTag` | server | server |
| `unscopables` | server | server |
| `dispose` | server | server |
| `asyncDispose` | server | server |
| `toString` | server | server |
| `valueOf` | server | server |
| `description` | server | server |

### BigInt

| Member | Loader | Component |
| --- | --- | --- |
| `asUintN` | throws, as in node | throws, as in node |
| `asIntN` | throws, as in node | throws, as in node |
| `toLocaleString` | server | server |
| `toString` | server | server |
| `valueOf` | throws, as in node | throws, as in node |

### Math

| Member | Loader | Component |
| --- | --- | --- |
| `abs` | server | server |
| `acos` | server | server |
| `acosh` | server | server |
| `asin` | server | server |
| `asinh` | server | server |
| `atan` | server | server |
| `atanh` | server | server |
| `atan2` | server | server |
| `ceil` | server | server |
| `cbrt` | server | server |
| `expm1` | server | server |
| `clz32` | server | server |
| `cos` | server | server |
| `cosh` | server | server |
| `exp` | server | server |
| `floor` | server | server |
| `fround` | server | server |
| `hypot` | server | server |
| `imul` | server | server |
| `log` | server | server |
| `log1p` | server | server |
| `log2` | server | server |
| `log10` | server | server |
| `max` | server | server |
| `min` | server | server |
| `pow` | server | server |
| `random` | server | browser |
| `round` | server | server |
| `sign` | server | server |
| `sin` | server | server |
| `sinh` | server | server |
| `sqrt` | server | server |
| `tan` | server | server |
| `tanh` | server | server |
| `trunc` | server | server |
| `E` | server | server |
| `LN10` | server | server |
| `LN2` | server | server |
| `LOG10E` | server | server |
| `LOG2E` | server | server |
| `PI` | server | server |
| `SQRT1_2` | server | server |
| `SQRT2` | server | server |
| `f16round` | server | server |

### JSON

| Member | Loader | Component |
| --- | --- | --- |
| `parse` | server | server |
| `stringify` | server | server |
| `rawJSON` | server | server |
| `isRawJSON` | server | server |

### Reflect

| Member | Loader | Component |
| --- | --- | --- |
| `defineProperty` | refused | refused |
| `deleteProperty` | server | server |
| `apply` | server | server |
| `construct` | server | server |
| `get` | server | server |
| `getOwnPropertyDescriptor` | server | server |
| `getPrototypeOf` | server | server |
| `has` | server | server |
| `isExtensible` | server | server |
| `ownKeys` | server | server |
| `preventExtensions` | server | server |
| `set` | server | server |
| `setPrototypeOf` | refused | refused |

### Date

| Member | Loader | Component |
| --- | --- | --- |
| `now` | server | browser |
| `parse` | server | server |
| `UTC` | server | server |
| `toString` | server | browser |
| `toDateString` | server | browser |
| `toTimeString` | server | browser |
| `toISOString` | server | server |
| `toUTCString` | server | server |
| `toGMTString` | server | server |
| `getDate` | server | browser |
| `setDate` | server | browser |
| `getDay` | server | browser |
| `getFullYear` | server | browser |
| `setFullYear` | server | browser |
| `getHours` | server | browser |
| `setHours` | server | browser |
| `getMilliseconds` | server | server |
| `setMilliseconds` | server | browser |
| `getMinutes` | server | browser |
| `setMinutes` | server | browser |
| `getMonth` | server | browser |
| `setMonth` | server | browser |
| `getSeconds` | server | browser |
| `setSeconds` | server | browser |
| `getTime` | server | server |
| `setTime` | server | server |
| `getTimezoneOffset` | server | browser |
| `getUTCDate` | server | server |
| `setUTCDate` | server | server |
| `getUTCDay` | server | server |
| `getUTCFullYear` | server | server |
| `setUTCFullYear` | server | server |
| `getUTCHours` | server | server |
| `setUTCHours` | server | server |
| `getUTCMilliseconds` | server | server |
| `setUTCMilliseconds` | server | server |
| `getUTCMinutes` | server | server |
| `setUTCMinutes` | server | server |
| `getUTCMonth` | server | server |
| `setUTCMonth` | server | server |
| `getUTCSeconds` | server | server |
| `setUTCSeconds` | server | server |
| `valueOf` | server | server |
| `getYear` | server | browser |
| `setYear` | server | browser |
| `toJSON` | server | server |
| `toLocaleString` | server | browser |
| `toLocaleDateString` | server | browser |
| `toLocaleTimeString` | server | browser |
| `toTemporalInstant` | server | server |

### RegExp

| Member | Loader | Component |
| --- | --- | --- |
| `input` | refused | refused |
| `$_` | refused | refused |
| `lastMatch` | refused | refused |
| `$&` | refused | refused |
| `lastParen` | refused | refused |
| `$+` | refused | refused |
| `leftContext` | refused | refused |
| `$`` | refused | refused |
| `rightContext` | refused | refused |
| `$'` | refused | refused |
| `$1` | refused | refused |
| `$2` | refused | refused |
| `$3` | refused | refused |
| `$4` | refused | refused |
| `$5` | refused | refused |
| `$6` | refused | refused |
| `$7` | refused | refused |
| `$8` | refused | refused |
| `$9` | refused | refused |
| `escape` | throws, as in node | throws, as in node |
| `exec` | server | server |
| `dotAll` | server | server |
| `flags` | server | server |
| `global` | server | server |
| `hasIndices` | server | server |
| `ignoreCase` | server | server |
| `multiline` | server | server |
| `source` | server | server |
| `sticky` | server | server |
| `unicode` | server | server |
| `unicodeSets` | server | server |
| `compile` | server | server |
| `toString` | server | server |
| `test` | server | server |

### Map

| Member | Loader | Component |
| --- | --- | --- |
| `groupBy` | server | server |
| `get` | server | server |
| `set` | server | server |
| `has` | server | server |
| `delete` | server | server |
| `clear` | server | server |
| `entries` | server | server |
| `forEach` | server | server |
| `keys` | server | server |
| `size` | server | server |
| `values` | server | server |
| `getOrInsert` | server | server |
| `getOrInsertComputed` | server | server |

### Set

| Member | Loader | Component |
| --- | --- | --- |
| `has` | server | server |
| `add` | server | server |
| `delete` | server | server |
| `difference` | server | server |
| `clear` | server | server |
| `entries` | server | server |
| `forEach` | server | server |
| `intersection` | server | server |
| `isSubsetOf` | server | server |
| `isSupersetOf` | server | server |
| `isDisjointFrom` | server | server |
| `size` | server | server |
| `symmetricDifference` | server | server |
| `union` | server | server |
| `values` | server | server |
| `keys` | server | server |

### WeakMap

| Member | Loader | Component |
| --- | --- | --- |
| `delete` | server | server |
| `get` | server | server |
| `set` | throws, as in node | throws, as in node |
| `has` | server | server |
| `getOrInsert` | server | server |
| `getOrInsertComputed` | server | server |

### WeakSet

| Member | Loader | Component |
| --- | --- | --- |
| `delete` | server | server |
| `has` | server | server |
| `add` | throws, as in node | throws, as in node |

### WeakRef

| Member | Loader | Component |
| --- | --- | --- |
| `deref` | server | server |

### FinalizationRegistry

| Member | Loader | Component |
| --- | --- | --- |
| `register` | server | server |
| `unregister` | server | server |

### Promise

| Member | Loader | Component |
| --- | --- | --- |
| `all` | server | loader only |
| `allSettled` | server | loader only |
| `any` | server | loader only |
| `race` | server | loader only |
| `resolve` | server | loader only |
| `reject` | throws, as in node | loader only |
| `withResolvers` | server | loader only |
| `try` | server | loader only |
| `then` | refused | loader only |
| `catch` | refused | loader only |
| `finally` | refused | loader only |

### Proxy

| Member | Loader | Component |
| --- | --- | --- |
| `revocable` | refused | refused |

### Error

| Member | Loader | Component |
| --- | --- | --- |
| `captureStackTrace` | refused | refused |
| `prepareStackTrace` | refused | refused |
| `isError` | server | server |
| `stackTraceLimit` | refused | refused |
| `name` | server | server |
| `message` | server | server |
| `toString` | server | server |

### ArrayBuffer

| Member | Loader | Component |
| --- | --- | --- |
| `isView` | server | server |
| `byteLength` | server | server |
| `slice` | server | server |
| `maxByteLength` | server | server |
| `resizable` | server | server |
| `resize` | refused | refused |
| `transfer` | refused | refused |
| `transferToFixedLength` | refused | refused |
| `detached` | server | server |

### DataView

| Member | Loader | Component |
| --- | --- | --- |
| `buffer` | server | server |
| `byteLength` | server | server |
| `byteOffset` | server | server |
| `getInt8` | server | server |
| `setInt8` | server | server |
| `getUint8` | server | server |
| `setUint8` | server | server |
| `getInt16` | server | server |
| `setInt16` | server | server |
| `getUint16` | server | server |
| `setUint16` | server | server |
| `getInt32` | server | server |
| `setInt32` | server | server |
| `getUint32` | server | server |
| `setUint32` | server | server |
| `getFloat32` | server | server |
| `setFloat32` | server | server |
| `getFloat64` | server | server |
| `setFloat64` | server | server |
| `getBigInt64` | throws, as in node | throws, as in node |
| `setBigInt64` | server | server |
| `getBigUint64` | throws, as in node | throws, as in node |
| `setBigUint64` | server | server |
| `getFloat16` | server | server |
| `setFloat16` | server | server |

### Uint8Array

| Member | Loader | Component |
| --- | --- | --- |
| `BYTES_PER_ELEMENT` | server | server |
| `fromBase64` | throws, as in node | throws, as in node |
| `fromHex` | throws, as in node | throws, as in node |
| `BYTES_PER_ELEMENT` | server | server |
| `toBase64` | server | server |
| `setFromBase64` | throws, as in node | throws, as in node |
| `toHex` | server | server |
| `setFromHex` | throws, as in node | throws, as in node |

### Iterator

| Member | Loader | Component |
| --- | --- | --- |
| `from` | server | server |
| `concat` | server | server |
| `reduce` | server | server |
| `toArray` | server | server |
| `forEach` | server | server |
| `some` | server | server |
| `every` | server | server |
| `find` | server | server |
| `map` | server | server |
| `filter` | server | server |
| `take` | server | server |
| `drop` | server | server |
| `flatMap` | server | server |

### Intl

| Member | Loader | Component |
| --- | --- | --- |
| `getCanonicalLocales` | server | server |
| `supportedValuesOf` | server | server |
| `DateTimeFormat` | server | server |
| `NumberFormat` | server | server |
| `Collator` | server | server |
| `PluralRules` | server | server |
| `RelativeTimeFormat` | server | server |
| `ListFormat` | server | server |
| `Locale` | server | server |
| `DisplayNames` | server | server |
| `Segmenter` | server | server |
| `DurationFormat` | server | server |

### Temporal

| Member | Loader | Component |
| --- | --- | --- |
| `Now` | server | browser |
| `PlainDate` | server | server |
| `PlainTime` | server | server |
| `PlainDateTime` | server | server |
| `ZonedDateTime` | server | server |
| `Duration` | server | server |
| `Instant` | server | server |
| `PlainYearMonth` | server | server |
| `PlainMonthDay` | server | server |

### URL

| Member | Loader | Component |
| --- | --- | --- |
| `parse` | server | server |
| `canParse` | server | server |
| `createObjectURL` | refused | refused |
| `revokeObjectURL` | refused | refused |
| `toString` | server | server |
| `href` | server | server |
| `origin` | server | server |
| `protocol` | server | server |
| `username` | server | server |
| `password` | server | server |
| `host` | server | server |
| `hostname` | server | server |
| `port` | server | server |
| `pathname` | server | server |
| `search` | server | server |
| `searchParams` | server | server |
| `hash` | server | server |
| `toJSON` | server | server |

### URLSearchParams

| Member | Loader | Component |
| --- | --- | --- |
| `size` | server | server |
| `append` | refused | refused |
| `delete` | refused | refused |
| `get` | server | server |
| `getAll` | server | server |
| `has` | server | server |
| `set` | refused | refused |
| `sort` | refused | refused |
| `entries` | server | server |
| `forEach` | server | server |
| `keys` | server | server |
| `values` | server | server |
| `toString` | server | server |

### TextEncoder

| Member | Loader | Component |
| --- | --- | --- |
| `encoding` | server | server |
| `encode` | server | server |
| `encodeInto` | refused | refused |

### TextDecoder

| Member | Loader | Component |
| --- | --- | --- |
| `decode` | server | server |
| `encoding` | server | server |
| `fatal` | server | server |
| `ignoreBOM` | server | server |

### console

| Member | Loader | Component |
| --- | --- | --- |
| `log` | refused | refused |
| `info` | refused | refused |
| `debug` | refused | refused |
| `warn` | refused | refused |
| `error` | refused | refused |
| `dir` | refused | refused |
| `time` | refused | refused |
| `timeEnd` | refused | refused |
| `timeLog` | refused | refused |
| `trace` | refused | refused |
| `assert` | refused | refused |
| `clear` | refused | refused |
| `count` | refused | refused |
| `countReset` | refused | refused |
| `group` | refused | refused |
| `groupEnd` | refused | refused |
| `table` | refused | refused |
| `dirxml` | refused | refused |
| `groupCollapsed` | refused | refused |
| `_stdoutErrorHandler` | refused | refused |
| `_stderrErrorHandler` | refused | refused |
| `_ignoreErrors` | refused | refused |
| `_times` | refused | refused |
| `Console` | refused | refused |
| `profile` | refused | refused |
| `profileEnd` | refused | refused |
| `timeStamp` | refused | refused |
| `context` | refused | refused |
| `createTask` | refused | refused |
| `_stdout` | refused | refused |
| `_stderr` | refused | refused |

### Atomics

| Member | Loader | Component |
| --- | --- | --- |
| `load` | refused | refused |
| `store` | refused | refused |
| `add` | refused | refused |
| `sub` | refused | refused |
| `and` | refused | refused |
| `or` | refused | refused |
| `xor` | refused | refused |
| `exchange` | refused | refused |
| `compareExchange` | refused | refused |
| `isLockFree` | refused | refused |
| `waitAsync` | refused | refused |
| `notify` | refused | refused |
| `pause` | refused | refused |

### DisposableStack

| Member | Loader | Component |
| --- | --- | --- |
| `use` | server | server |
| `dispose` | server | server |
| `adopt` | server | server |
| `defer` | server | server |
| `move` | server | server |
| `disposed` | server | server |

### AsyncDisposableStack

| Member | Loader | Component |
| --- | --- | --- |
| `use` | server | server |
| `disposeAsync` | server | loader only |
| `adopt` | server | server |
| `defer` | server | server |
| `move` | server | server |
| `disposed` | server | server |

### SharedArrayBuffer

| Member | Loader | Component |
| --- | --- | --- |
| `byteLength` | server | server |
| `slice` | server | server |
| `maxByteLength` | server | server |
| `growable` | server | server |
| `grow` | refused | refused |

### Headers

| Member | Loader | Component |
| --- | --- | --- |
| `append` | refused | refused |
| `delete` | refused | refused |
| `get` | refused | refused |
| `has` | refused | refused |
| `set` | refused | refused |
| `getSetCookie` | refused | refused |
| `keys` | refused | refused |
| `values` | refused | refused |
| `entries` | refused | refused |
| `forEach` | refused | refused |

### Response

| Member | Loader | Component |
| --- | --- | --- |
| `error` | refused | refused |
| `json` | server | loader only |
| `redirect` | server | server |
| `type` | server | server |
| `url` | server | server |
| `redirected` | server | server |
| `status` | server | server |
| `ok` | server | server |
| `statusText` | server | server |
| `headers` | refused | refused |
| `body` | refused | refused |
| `bodyUsed` | refused | refused |
| `clone` | refused | refused |
| `blob` | refused | refused |
| `arrayBuffer` | refused | refused |
| `text` | refused | refused |
| `json` | server | loader only |
| `formData` | refused | refused |
| `bytes` | refused | refused |
| `textStream` | refused | refused |

### Request

| Member | Loader | Component |
| --- | --- | --- |
| `method` | server | server |
| `url` | server | server |
| `headers` | refused | refused |
| `destination` | server | server |
| `referrer` | server | server |
| `referrerPolicy` | server | server |
| `mode` | server | server |
| `credentials` | server | server |
| `cache` | server | server |
| `redirect` | server | server |
| `integrity` | server | server |
| `keepalive` | server | server |
| `isReloadNavigation` | server | server |
| `isHistoryNavigation` | server | server |
| `signal` | refused | refused |
| `body` | refused | refused |
| `bodyUsed` | refused | refused |
| `duplex` | server | server |
| `clone` | refused | refused |
| `blob` | refused | refused |
| `arrayBuffer` | refused | refused |
| `text` | refused | refused |
| `json` | refused | refused |
| `formData` | refused | refused |
| `bytes` | refused | refused |
| `textStream` | refused | refused |
| `attribute` | refused | refused |

### AbortController

| Member | Loader | Component |
| --- | --- | --- |
| `signal` | refused | refused |
| `abort` | refused | refused |

### Intl.Collator

| Member | Loader | Component |
| --- | --- | --- |
| `supportedLocalesOf` | server | server |
| `resolvedOptions` | server | server |
| `compare` | server | server |

### Intl.DateTimeFormat

| Member | Loader | Component |
| --- | --- | --- |
| `supportedLocalesOf` | server | server |
| `resolvedOptions` | server | server |
| `formatToParts` | server | server |
| `format` | server | server |
| `formatRange` | refused | refused |
| `formatRangeToParts` | refused | refused |

### Intl.DisplayNames

| Member | Loader | Component |
| --- | --- | --- |
| `supportedLocalesOf` | server | server |
| `resolvedOptions` | server | server |
| `of` | server | server |

### Intl.DurationFormat

| Member | Loader | Component |
| --- | --- | --- |
| `supportedLocalesOf` | server | server |
| `resolvedOptions` | server | server |
| `format` | server | server |
| `formatToParts` | refused | refused |

### Intl.ListFormat

| Member | Loader | Component |
| --- | --- | --- |
| `supportedLocalesOf` | server | server |
| `resolvedOptions` | server | server |
| `format` | server | server |
| `formatToParts` | server | server |

### Intl.Locale

| Member | Loader | Component |
| --- | --- | --- |
| `toString` | server | server |
| `maximize` | server | server |
| `minimize` | server | server |
| `language` | server | server |
| `script` | server | server |
| `region` | server | server |
| `baseName` | server | server |
| `calendar` | server | server |
| `caseFirst` | server | server |
| `collation` | server | server |
| `firstDayOfWeek` | server | server |
| `hourCycle` | server | server |
| `numeric` | server | server |
| `numberingSystem` | server | server |
| `getCalendars` | server | server |
| `getCollations` | refused | refused |
| `getHourCycles` | server | server |
| `getNumberingSystems` | server | server |
| `getTimeZones` | server | server |
| `getTextInfo` | server | server |
| `getWeekInfo` | server | server |

### Intl.NumberFormat

| Member | Loader | Component |
| --- | --- | --- |
| `supportedLocalesOf` | server | server |
| `resolvedOptions` | server | server |
| `formatToParts` | server | server |
| `format` | server | server |
| `formatRange` | refused | refused |
| `formatRangeToParts` | refused | refused |

### Intl.PluralRules

| Member | Loader | Component |
| --- | --- | --- |
| `supportedLocalesOf` | server | server |
| `resolvedOptions` | server | server |
| `select` | server | server |
| `selectRange` | refused | refused |

### Intl.RelativeTimeFormat

| Member | Loader | Component |
| --- | --- | --- |
| `supportedLocalesOf` | server | server |
| `resolvedOptions` | server | server |
| `format` | server | server |
| `formatToParts` | server | server |

### Intl.Segmenter

| Member | Loader | Component |
| --- | --- | --- |
| `supportedLocalesOf` | server | server |
| `resolvedOptions` | server | server |
| `segment` | server | browser |

### Temporal.PlainDate

| Member | Loader | Component |
| --- | --- | --- |
| `from` | server | server |
| `compare` | server | server |
| `era` | server | server |
| `eraYear` | server | server |
| `calendarId` | server | server |
| `year` | server | server |
| `month` | server | server |
| `monthCode` | server | server |
| `day` | server | server |
| `dayOfWeek` | server | server |
| `dayOfYear` | server | server |
| `weekOfYear` | server | server |
| `yearOfWeek` | server | server |
| `daysInWeek` | server | server |
| `daysInMonth` | server | server |
| `daysInYear` | server | server |
| `monthsInYear` | server | server |
| `inLeapYear` | server | server |
| `toPlainYearMonth` | server | server |
| `toPlainMonthDay` | server | server |
| `add` | throws, as in node | throws, as in node |
| `subtract` | throws, as in node | throws, as in node |
| `with` | throws, as in node | throws, as in node |
| `withCalendar` | throws, as in node | throws, as in node |
| `until` | server | server |
| `since` | server | server |
| `equals` | server | server |
| `toPlainDateTime` | server | server |
| `toZonedDateTime` | throws, as in node | throws, as in node |
| `toString` | server | server |
| `toLocaleString` | server | server |
| `toJSON` | server | server |
| `valueOf` | throws, as in node | throws, as in node |

### Temporal.PlainTime

| Member | Loader | Component |
| --- | --- | --- |
| `from` | server | server |
| `compare` | server | server |
| `hour` | server | server |
| `minute` | server | server |
| `second` | server | server |
| `millisecond` | server | server |
| `microsecond` | server | server |
| `nanosecond` | server | server |
| `add` | throws, as in node | throws, as in node |
| `subtract` | throws, as in node | throws, as in node |
| `with` | throws, as in node | throws, as in node |
| `until` | throws, as in node | throws, as in node |
| `since` | throws, as in node | throws, as in node |
| `round` | throws, as in node | throws, as in node |
| `equals` | throws, as in node | throws, as in node |
| `toLocaleString` | server | server |
| `toString` | server | server |
| `toJSON` | server | server |
| `valueOf` | throws, as in node | throws, as in node |

### Temporal.PlainDateTime

| Member | Loader | Component |
| --- | --- | --- |
| `from` | server | server |
| `compare` | server | server |
| `calendarId` | server | server |
| `era` | server | server |
| `eraYear` | server | server |
| `year` | server | server |
| `month` | server | server |
| `monthCode` | server | server |
| `day` | server | server |
| `hour` | server | server |
| `minute` | server | server |
| `second` | server | server |
| `millisecond` | server | server |
| `microsecond` | server | server |
| `nanosecond` | server | server |
| `dayOfWeek` | server | server |
| `dayOfYear` | server | server |
| `weekOfYear` | server | server |
| `yearOfWeek` | server | server |
| `daysInWeek` | server | server |
| `daysInMonth` | server | server |
| `daysInYear` | server | server |
| `monthsInYear` | server | server |
| `inLeapYear` | server | server |
| `with` | throws, as in node | throws, as in node |
| `withCalendar` | throws, as in node | throws, as in node |
| `withPlainTime` | server | server |
| `add` | throws, as in node | throws, as in node |
| `subtract` | throws, as in node | throws, as in node |
| `until` | server | server |
| `since` | server | server |
| `round` | throws, as in node | throws, as in node |
| `equals` | server | server |
| `toLocaleString` | server | server |
| `toJSON` | server | server |
| `toString` | server | server |
| `valueOf` | throws, as in node | throws, as in node |
| `toZonedDateTime` | throws, as in node | throws, as in node |
| `toPlainDate` | server | server |
| `toPlainTime` | server | server |

### Temporal.ZonedDateTime

| Member | Loader | Component |
| --- | --- | --- |
| `from` | server | server |
| `compare` | server | server |
| `timeZoneId` | server | server |
| `calendarId` | server | server |
| `era` | server | server |
| `eraYear` | server | server |
| `year` | server | server |
| `month` | server | server |
| `monthCode` | server | server |
| `day` | server | server |
| `hour` | server | server |
| `minute` | server | server |
| `second` | server | server |
| `millisecond` | server | server |
| `microsecond` | server | server |
| `nanosecond` | server | server |
| `epochMilliseconds` | server | server |
| `epochNanoseconds` | throws, as in node | throws, as in node |
| `dayOfWeek` | server | server |
| `dayOfYear` | server | server |
| `weekOfYear` | server | server |
| `yearOfWeek` | server | server |
| `hoursInDay` | server | server |
| `daysInWeek` | server | server |
| `daysInMonth` | server | server |
| `daysInYear` | server | server |
| `monthsInYear` | server | server |
| `inLeapYear` | server | server |
| `offsetNanoseconds` | server | server |
| `offset` | server | server |
| `with` | throws, as in node | throws, as in node |
| `withCalendar` | throws, as in node | throws, as in node |
| `withPlainTime` | server | server |
| `withTimeZone` | throws, as in node | throws, as in node |
| `add` | throws, as in node | throws, as in node |
| `subtract` | throws, as in node | throws, as in node |
| `until` | throws, as in node | throws, as in node |
| `since` | throws, as in node | throws, as in node |
| `round` | throws, as in node | throws, as in node |
| `equals` | throws, as in node | throws, as in node |
| `toLocaleString` | server | server |
| `toString` | server | server |
| `toJSON` | server | server |
| `valueOf` | throws, as in node | throws, as in node |
| `startOfDay` | server | server |
| `getTimeZoneTransition` | throws, as in node | throws, as in node |
| `toInstant` | server | server |
| `toPlainDate` | server | server |
| `toPlainTime` | server | server |
| `toPlainDateTime` | server | server |

### Temporal.Duration

| Member | Loader | Component |
| --- | --- | --- |
| `from` | server | server |
| `compare` | server | server |
| `years` | server | server |
| `months` | server | server |
| `weeks` | server | server |
| `days` | server | server |
| `hours` | server | server |
| `minutes` | server | server |
| `seconds` | server | server |
| `milliseconds` | server | server |
| `microseconds` | server | server |
| `nanoseconds` | server | server |
| `sign` | server | server |
| `blank` | server | server |
| `with` | throws, as in node | throws, as in node |
| `negated` | server | server |
| `abs` | server | server |
| `add` | throws, as in node | throws, as in node |
| `subtract` | throws, as in node | throws, as in node |
| `round` | throws, as in node | throws, as in node |
| `total` | throws, as in node | throws, as in node |
| `toLocaleString` | server | server |
| `toString` | server | server |
| `toJSON` | server | server |
| `valueOf` | throws, as in node | throws, as in node |

### Temporal.Instant

| Member | Loader | Component |
| --- | --- | --- |
| `from` | server | server |
| `fromEpochMilliseconds` | server | server |
| `fromEpochNanoseconds` | server | server |
| `compare` | server | server |
| `epochMilliseconds` | server | server |
| `epochNanoseconds` | throws, as in node | throws, as in node |
| `add` | throws, as in node | throws, as in node |
| `subtract` | throws, as in node | throws, as in node |
| `until` | throws, as in node | throws, as in node |
| `since` | throws, as in node | throws, as in node |
| `round` | throws, as in node | throws, as in node |
| `equals` | throws, as in node | throws, as in node |
| `toLocaleString` | server | server |
| `toString` | server | server |
| `toJSON` | server | server |
| `valueOf` | throws, as in node | throws, as in node |
| `toZonedDateTimeISO` | throws, as in node | throws, as in node |

### Temporal.PlainYearMonth

| Member | Loader | Component |
| --- | --- | --- |
| `from` | server | server |
| `compare` | server | server |
| `calendarId` | server | server |
| `era` | server | server |
| `eraYear` | server | server |
| `year` | server | server |
| `month` | server | server |
| `monthCode` | server | server |
| `daysInYear` | server | server |
| `daysInMonth` | server | server |
| `monthsInYear` | server | server |
| `inLeapYear` | server | server |
| `with` | throws, as in node | throws, as in node |
| `add` | throws, as in node | throws, as in node |
| `subtract` | throws, as in node | throws, as in node |
| `until` | server | server |
| `since` | server | server |
| `equals` | server | server |
| `toLocaleString` | throws, as in node | throws, as in node |
| `toString` | server | server |
| `toJSON` | server | server |
| `valueOf` | throws, as in node | throws, as in node |
| `toPlainDate` | throws, as in node | throws, as in node |

### Temporal.PlainMonthDay

| Member | Loader | Component |
| --- | --- | --- |
| `from` | server | server |
| `calendarId` | server | server |
| `monthCode` | server | server |
| `day` | server | server |
| `with` | throws, as in node | throws, as in node |
| `equals` | server | server |
| `toLocaleString` | throws, as in node | throws, as in node |
| `toString` | server | server |
| `toJSON` | server | server |
| `valueOf` | throws, as in node | throws, as in node |
| `toPlainDate` | throws, as in node | throws, as in node |

### Temporal.Now

| Member | Loader | Component |
| --- | --- | --- |
| `instant` | server | server |
| `timeZoneId` | server | browser |
| `plainDateTimeISO` | server | server |
| `zonedDateTimeISO` | server | server |
| `plainDateISO` | server | server |
| `plainTimeISO` | server | server |

### Iterator.prototype

| Member | Loader | Component |
| --- | --- | --- |
| `constructor` | server | server |
| `reduce` | server | server |
| `toArray` | server | server |
| `forEach` | server | server |
| `some` | server | server |
| `every` | server | server |
| `find` | server | server |
| `map` | server | server |
| `filter` | server | server |
| `take` | server | server |
| `drop` | server | server |
| `flatMap` | server | server |

### TypedArray

| Member | Loader | Component |
| --- | --- | --- |
| `of` | server | server |
| `from` | server | server |
| `buffer` | server | server |
| `byteLength` | server | server |
| `byteOffset` | server | server |
| `length` | server | server |
| `entries` | server | server |
| `keys` | server | server |
| `values` | server | server |
| `at` | server | server |
| `copyWithin` | server | server |
| `every` | server | server |
| `fill` | server | server |
| `filter` | server | server |
| `find` | server | server |
| `findIndex` | server | server |
| `findLast` | server | server |
| `findLastIndex` | server | server |
| `forEach` | server | server |
| `includes` | server | server |
| `indexOf` | server | server |
| `join` | server | server |
| `lastIndexOf` | server | server |
| `map` | server | server |
| `reverse` | server | server |
| `reduce` | server | server |
| `reduceRight` | server | server |
| `set` | server | server |
| `slice` | server | server |
| `some` | server | server |
| `sort` | server | server |
| `subarray` | server | server |
| `toReversed` | server | server |
| `toSorted` | server | server |
| `with` | server | server |
| `toLocaleString` | server | server |
| `toString` | server | server |

### structuredClone

| Member | Loader | Component |
| --- | --- | --- |
| `structuredClone` | server | server |

### atob

| Member | Loader | Component |
| --- | --- | --- |
| `atob` | server | server |

### btoa

| Member | Loader | Component |
| --- | --- | --- |
| `btoa` | server | server |

### parseInt

| Member | Loader | Component |
| --- | --- | --- |
| `parseInt` | server | server |

### parseFloat

| Member | Loader | Component |
| --- | --- | --- |
| `parseFloat` | server | server |

### isNaN

| Member | Loader | Component |
| --- | --- | --- |
| `isNaN` | server | server |

### isFinite

| Member | Loader | Component |
| --- | --- | --- |
| `isFinite` | server | server |

### encodeURI

| Member | Loader | Component |
| --- | --- | --- |
| `encodeURI` | server | server |

### encodeURIComponent

| Member | Loader | Component |
| --- | --- | --- |
| `encodeURIComponent` | server | server |

### decodeURI

| Member | Loader | Component |
| --- | --- | --- |
| `decodeURI` | server | server |

### decodeURIComponent

| Member | Loader | Component |
| --- | --- | --- |
| `decodeURIComponent` | server | server |

### Object

| Member | Loader | Component |
| --- | --- | --- |
| `Object()` | server | server |

### Array

| Member | Loader | Component |
| --- | --- | --- |
| `Array()` | server | server |

### Number

| Member | Loader | Component |
| --- | --- | --- |
| `Number()` | server | server |

### Boolean

| Member | Loader | Component |
| --- | --- | --- |
| `Boolean()` | server | server |

### String

| Member | Loader | Component |
| --- | --- | --- |
| `String()` | server | server |

### Symbol

| Member | Loader | Component |
| --- | --- | --- |
| `Symbol()` | server | server |

### BigInt

| Member | Loader | Component |
| --- | --- | --- |
| `BigInt()` | throws, as in node | throws, as in node |

### Date

| Member | Loader | Component |
| --- | --- | --- |
| `Date()` | server | browser |

### RegExp

| Member | Loader | Component |
| --- | --- | --- |
| `RegExp()` | server | server |

### Error

| Member | Loader | Component |
| --- | --- | --- |
| `Error()` | server | server |

### Function

| Member | Loader | Component |
| --- | --- | --- |
| `Function()` | server | server |

<!-- /fsr:members -->
