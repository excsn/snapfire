# uploads_react_ts

A file posted to an action, twice: once by a form the browser submits natively and once by the same action called with a `FormData` from the page. The host reads `multipart/form-data` into the input an action already takes, so a file part arrives as an `Upload` beside the text fields and the body validates it like anything else.

| It shows | Where |
| --- | --- |
| A schema field declared as the built-in `Upload` | `file: Upload` in `app/schemas/upload.ts` |
| An action reading a file's name, type and length | `routes/actions.ts` |
| A form that posts a file with no JavaScript at all | `<form encType="multipart/form-data">` in `routes/page.tsx` |
| The same action called from the page with a `FormData` | `upload("$root.deposit", data)` in the `Deposit` island |
| A part refused before the body runs | `max_upload` in `config/app.toml` |
| A type the application refuses itself | `ACCEPTED` in `routes/actions.ts` |
| A `fail` that lands back on the form rather than on a page of JSON | the empty-file guard in `routes/actions.ts`, read as `action_failure` in `routes/page.tsx` |
| The per-kind error boundary | `routes/error.not-found.tsx` beside `routes/error.tsx` |

## Run it

```sh
fsr build app
fsr serve app
```

Then open `http://127.0.0.1:8108`.

## What arrives

A part with a filename becomes an `Upload`:

```ts
interface Upload {
  filename: string;
  content_type: string;
  size: bigint;
  bytes: Uint8Array;
}
```

`filename` and `content_type` are what the browser claimed, so neither is trustworthy on its own. The action checks the type against a list it holds and the host has already refused anything over `server.max_upload`. A part with no filename is an ordinary text field, coerced against the schema exactly as a urlencoded form field is.

`bytes` is the file. This example does not keep it: it records what arrived and drops the rest, because an example with no storage behind it has nowhere honest to put a file. A real application hands `bytes` to a service method or to its own Rust through `ctx.native`, which is where writing a file belongs.

## Two callers, one action

The form posts natively to `/_sf/action/$root.deposit`. A form post is answered with a redirect back to the page that posted, so the browser lands on the page with the session written and the table filled in. Nothing on that path needs JavaScript.

The island posts the same action with a `FormData` through `upload`, which names JSON in `Accept`, so the host answers the action's value instead of the redirect and the page refreshes in place. Both carry `_csrf`, since a form post is verified where a JSON call is not.

A `fail` follows the same split. The browser is redirected back with the failure waiting as the `action_failure` prop, which the page renders and which the next render does not, so a reload is clean. The JSON caller gets `{kind, message}` and the failure's status, which is what `upload` throws as an `ActionFailure`.

## What it costs

The request is buffered before anything parses it, so an upload is bounded by `server.max_body` and one part by `server.max_upload`. This is for the file a person picks in a form, not for a multi-gigabyte transfer. Raising `max_body` to allow one raises what a request may hold in memory.
