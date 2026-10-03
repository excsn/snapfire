# gallery_react_ts

Four evenings of phone photos on a wall. The example exists for its images and fonts: every path the build has for an asset is on one of its two pages, so a change to the pipeline shows up here before it shows up anywhere else.

## Running it

```sh
cargo run -p gallery_react_ts
```

Then open http://127.0.0.1:8200/. The images and fonts are served from `http://localhost:8200/`, which is the same process under its other name: the configuration names it as the base for both, so the markup carries absolute URLs to a second origin the way it would with a CDN in front; the browser fetches the fonts across origins the way it would there too.

## What is on show

| On the page | The path it exercises |
| --- | --- |
| Four photos stored on their side, shown upright | A phone writes the sensor's frame with an EXIF orientation. The build reads the tag, writes the displayed width and height on the element and rotates every variant, since a variant carries no tag. The original is served as saved and the browser rotates it. |
| The lead photo at the top of the wall | `priority`: eager, `fetchpriority="high"` and a preload in the head. |
| The four on the wall, by file name from the loader | A record of imports indexed by a value, one branch per entry, lazy, at the width the column renders. |
| The logo in the masthead | An SVG through `Picture` is served as it is, with its size and no `<picture>`. |
| The badge beside the photographer's name | `data-sf-raw` on a plain `img` keeps it plain. |
| The ridge photo on the about page as a plain `img` | The `<img>` rewrite: an imported image on a plain element becomes a `<picture>` like one placed with `Picture`. |
| The avatar on the about page | A remote source: `[images.sources.picsum]` is a URL template and the value from the loader is an id on that service, so the `srcset` names the service at every policy width and no byte passes through the build. The browser fetches it from the internet. |
| The wall's paper texture and the caption face | A stylesheet under `src/`, compiled by the bundle and linked from `[document] styles`: its `url()` to an image and to a font file outside the fonts directory are references the lowering never sees, so the compiler reports them, the build defines them and places the files. |
| The close button on a zoomed photo | `src/ui/zoom.ts` runs in the browser only and imports `close.png`; the same handshake defines it. |
| Fraunces, the display face | Vendored with `fsr fonts add app google:Fraunces@400,700`. The provider answers both weights with one variable file per subset, so three files land with their `.range` sidecars and the build declares `font-weight: 100 900` off the file's own axis. The fallback face is sized from Georgia against the Latin subset, since the Vietnamese one has no `a` to `z` to measure. |
| Inter, the body face | `[fonts.sans] remote`: the provider's stylesheet is linked, the preconnects are written, the variable falls back to a plain system stack and the text reflows when the face arrives. Every visitor's IP reaches the provider; vendoring is the other choice. |
| Every asset URL | `[images] base` and `[fonts] base`: the second origin, widened into the policy and sent `Access-Control-Allow-Origin` by the host. |

## Checking it

`cargo test -p gallery_react_ts` renders both pages in-process and asserts the markup for each row above. In a browser, open the wall with the network panel: the four photos come as AVIF at the column's width from `localhost:8200`, the lead was requested by the preload, Fraunces arrives from the same origin with `unicode-range` splitting the subsets and the captions use the face the stylesheet declared. Click a photo for the zoom and its close icon, which only the browser's build knew about.
