# notes_desktop_ts

A notes reader that runs in a desktop window. The loader reads the notes under `app/notes/` from disk with `file`. In the window, one button opens the system's file dialog and another opens a link in the system's browser, both through `@snapfire/fsr/desktop`.

| It shows | Where |
| --- | --- |
| A loader listing and reading files | `file.list` and `file.read` in `routes/page.loader.ts` |
| The directories a host may read | `[files] roots` in `config/app.toml` |
| A path outside those directories refused, with the reason on the page | `/?path=/etc/hosts` |
| The system's open dialog | `openFile` in `src/picker.tsx` |
| A link handed to the system's browser | `openExternal` in `src/picker.tsx` |
| The window's name and menu | `[desktop] name` in `config/app.toml` |

## Run it

An `fsr` built with the `desktop` feature opens the window:

```sh
cargo install snapfire_fsr_cli --features desktop
fsr dev app --desktop
```

For the production build:

```sh
fsr build app
fsr serve app --desktop
```

Without `--desktop` the same page serves at `http://127.0.0.1:8240`. The notes still read and each button says it runs only in an `fsr --desktop` window.

## Opening a note

`openFile` answers the paths the user chose. It answers `null` when the dialog is cancelled. The page navigates to `/?path=<path>` and the loader reads it:

```ts
const paths = await openFile({ filters: [{ name: "Notes", extensions: ["md", "txt"] }] });
if (paths) navigate(`/?path=${encodeURIComponent(paths[0])}`);
```

The dialog can pick any file, but the host reads only under `[files] roots`. A note picked from `app/notes/` opens. A file anywhere else shows the refusal the loader caught.
