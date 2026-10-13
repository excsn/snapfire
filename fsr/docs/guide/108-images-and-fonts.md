# 108. Images and fonts

**For app developers.** How an image a component imports becomes a `<picture>` served in the sizes a page needs, how a font under `fonts/` reaches the page with no layout shift and what each of the two costs.

## An image is an import

In a component, import the file and place it with `Picture`:

```tsx
import { Picture } from "@snapfire/fsr-client/react";
import hero from "../img/hero.png";

export default function Page() {
  return <Picture src={hero} alt="The harbour at dusk" sizes="(min-width: 60em) 50vw, 100vw" priority />;
}
```

The import is not a string. The bundle binds `hero` to the hashed URL the file is served under and the width and height it read from the file's header, so the browser module and the server agree on every number. The build reads the same import to write the markup the server renders:

```html
<picture>
  <source type="image/avif" srcset="/static/js/app/img/hero.3f2a9c1e.640.avif 640w, …, /static/js/app/img/hero.3f2a9c1e.1600.avif 1600w" sizes="(min-width: 60em) 50vw, 100vw">
  <source type="image/webp" srcset="…">
  <img src="/static/js/app/img/hero.3f2a9c1e.png" width="1600" height="900" alt="The harbour at dusk" loading="eager" decoding="async" fetchpriority="high">
</picture>
```

Three things follow from the markup. The browser picks the width that fits the slot from `srcset`, so a 300px card fetches the 640 variant and a phone at two pixels per CSS pixel fetches 960. `width` and `height` are on the element before any byte of the image arrives, so the page lays out once. The URL carries the file's hash, so the host serves it with a year's `immutable` lifetime and a changed file gets a new name.

`priority` is for the one image that is the largest thing above the fold. It loads eagerly at high priority and the page carries a `<link rel="preload">` for it in the head, which the browser fetches before the stylesheet parses. Everything else is lazy.

## Where the variants come from

`fsr build` reads every image a component imports, chooses its widths under the `[images]` policy and, after the bundle runs, writes the hashed original and the variants under `dist/`. The original a browser without AVIF or WebP falls back to is the file as saved less its metadata: GPS position, camera, capture time and an embedded thumbnail go, while the orientation the browser rotates by, the colour profile and the EXIF creator and copyright stay. No pixel is re-encoded. That holds for JPEG, PNG, WebP and GIF; `fsr doctor` names an AVIF original that carries metadata, since the build leaves one as saved. `strip = false` under `[images]` serves every original as saved. The bundle places nothing of its own: the build hands the compiler the URL of every image and font it defined. An import the build had not seen, from a module only the browser runs, comes back to it, is read and gets its variants like the rest:

```toml
[images]
widths = [640, 960, 1280, 1920, 2560]
formats = ["avif", "webp"]
quality = { avif = 60, webp = 80 }
```

Those are the defaults; an application that writes nothing gets them. An image gets every policy width below its own, then its own, so a 400px logo gets one variant and a 4000px hero six. Nothing is ever upscaled. Each variant is named with the source's hash, so the second build writes nothing and a dev save costs nothing after the first; the report says what it wrote:

```text
image     src/img/hero.png                   1600x900, 8 variants
derived   8 files under dist/, 0 already there
```

An SVG scales itself and an animated GIF would have every frame re-encoded, so both are served as they are, with width and height and no `<picture>`. An SVG's size is its root element's `width` and `height`, with its `viewBox` taking over when those are missing or relative; one with neither has no size to write and is refused.

A photo straight off a phone is stored on its side with an EXIF orientation tag the browser rotates by. The build reads the tag, so `width` and `height` are the size the photo displays at and every variant is written upright, since a variant carries no tag.

## A catalog rendered from data

A page rendered from a service's answer does not know at build time which image it shows. Write the record of imports and index it by the value:

```tsx
import one from "../img/products/1.png";
import two from "../img/products/2.png";

export const PHOTOS = { "1.png": one, "2.png": two };

export function Thumb({ image }: { image: Image }) {
  return <Picture src={PHOTOS[image.file]} alt="" sizes="300px" />;
}
```

The build lowers one branch per entry, so the server writes the right `<picture>` for the value it was given and the browser, holding the same record, writes the same one. A key no entry has renders nothing.

Such a record asks for no preload, since the key is a value. The page's loader knows the value, so its `meta` names the one to preload, on a route that is not streamed:

```ts
import { preloadImage } from "@snapfire/fsr/head";
import { PHOTOS } from "@src/ui/photos";

export const meta = ({ data }: MetaCtx<DataOf<typeof load>>) => ({
  title: "Today's picks",
  head: data.products.length > 0 ? [preloadImage(PHOTOS[data.products[0].image.file], "300px")] : [],
});
```

A route with a `loading.tsx` streams its page after the head is sent, so a preload named by its loader arrives with the segment, too late to matter. Mark the image `priority` instead and let `fetchpriority` do the work when the segment lands.

## A remote image

An image on another origin, a CMS or an object store, is a string `src`. Through a named source it gets a `srcset` the service answers:

```toml
[images.sources.cms]
template = "https://img.example.com/{src}?w={width}&auto=format"
```

```tsx
<Picture src={product.photo} source="cms" alt="" width={800} height={600} />
```

The build writes one URL per policy width with `{src}` and `{width}` filled. No byte passes through fsr. The service resizes, which an image CDN or a CMS already does. It negotiates the format when the template asks it to. Width and height are the author's, since there is no file to read. A string with no `source` is an `<img src>` as written.

## A font is a file under `fonts/`

Put the faces under `app/fonts/` and name the family:

```toml
[fonts.sans]
family = "Inter"
fallback = "Arial"
```

```css
body { font-family: var(--font-sans); }
```

The build reads each file's own tables for its weight and style, writes the `@font-face` rules and defines `--font-sans` as `"Inter", "Inter Fallback", sans-serif`. `Inter Fallback` is Arial drawn with `size-adjust` and the three overrides computed from the two faces' metrics, so a line of Arial takes the room the same line of Inter will. The text is laid out once in the fallback and the swap moves nothing. The regular weight is preloaded; the rest arrive when the CSS asks for them.

Every page carries the CSS inline, hashed into `style-src`, the preloads and the files under their hashed names. The host writes all of it; a layout has nothing to add.

To take a family from Google Fonts, fetch it once:

```sh
fsr fonts add app google:Inter@400,700
```

Each subset the provider serves lands as a file with a `.range` sidecar. When the file's own name table calls the family something else, as Bricolage Grotesque's does after its default optical size, a `.family` sidecar keeps the provider's name, so `family = "Bricolage Grotesque"` still finds it. A family the provider holds as a variable font comes as one file per subset whatever weights you asked for; the build declares it at the file's own weight axis, `font-weight: 100 900`, so every weight in between is yours too. The fallback is sized from the Latin subset, since a Vietnamese or Cyrillic one has no `a` to `z` to measure. The build serves them as local files from then on. The build itself never reaches the network. The other shape is a provider's own stylesheet, linked rather than served:

```toml
[fonts.display]
family = "Fraunces"
remote = "https://fonts.googleapis.com/css2?family=Fraunces&display=swap"
```

That key gets no fallback metrics, since there is no file to read, so its text reflows when the face arrives. Every visitor's IP goes to the provider on every page. Vendor when you can.

## A video or an audio file

A video is a plain `<video>` element. Import its files and the build serves each as it is under a content hash, the way it serves a font:

```tsx
import walkMp4 from "@src/media/walk.mp4";
import walkWebm from "@src/media/walk.webm";
import walkCaptions from "@src/media/walk.vtt";
import walkPoster from "@src/media/walk-poster.png";

<video controls preload="metadata" playsInline width={480} height={270} poster={walkPoster.src}>
  <source src={walkWebm} type="video/webm" />
  <source src={walkMp4} type="video/mp4" />
  <track kind="captions" src={walkCaptions} srcLang="en" label="English" default />
</video>
```

The conference app's venue page has this one. The import is the URL, so it works the same in a lowered component and in an island. `mp4`, `m4v`, `webm`, `ogv` and `mov` are video, `mp3`, `m4a`, `aac`, `oga`, `ogg`, `opus` and `wav` are audio and `vtt` is a caption track. The host answers a `Range` request with `206`, which a browser needs to seek and Safari needs to play at all. Nothing is transcoded and no poster is cut from the file: encode the sources you want, WebM first and MP4 as the fallback. The poster is an image you give.

Set `width` and `height` so the page does not move when the video loads. Set `preload="metadata"` so a visitor who never plays it does not download it. A video that plays on its own needs `muted` and `playsInline` as well as `autoPlay`, since browsers refuse to autoplay sound. A `src` written as a literal URL on another origin, a CDN or a hosting service, is added to the policy's `media-src`.

## A video hosted elsewhere

A YouTube or Vimeo video is an `Embed`. The server renders the provider's still with the title and a play button. The player's iframe replaces it when the visitor clicks:

```tsx
import { Embed } from "@snapfire/fsr-client/react";

<Embed provider="youtube" id="f_WuRfuMXQw" title="Rick Astley - Never Gonna Give You Up" />
```

`provider` and `id` are strings written out. `title` is required, since it names the video on the placeholder, to a screen reader and on the player. The placeholder keeps a 16:9 box, so nothing moves when it loads or plays; place it inside an element as wide as you want it and style that element through `className`. The still loads from the provider by default. `[media] embed_stills = "local"` fetches it at build and serves it from your origin instead, which chapter 306 explains along with everything else a page asks other hosts for. An `Embed` renders on the server, so place it in a page, a layout or a component that does not hydrate: the build refuses one inside a component the browser hydrates, which has no still to render it with.

## A card image beside a route

A link pasted into a chat or a post shows a card. The card's picture is the page's `og:image`. Put the picture beside the route it stands for:

```text
routes/
  opengraph-image.png            every page's card
  blog/
    opengraph-image.png          the blog's, replacing it under /blog
    opengraph-image.alt.txt      its alt text
    twitter-image.png            a separate picture for X's card
```

The build serves each as its hashed original with no resized variant, since a crawler fetches one URL. It strips the metadata as `[images] strip` says. It writes `og:image` with its width, height and type and `og:image:alt` from the text file. A `twitter-image` gets the same rows under `twitter:image`. X reads the Open Graph image when there is no `twitter-image`, so one file is usually enough. A page sets its own card type with `twitter("card", "summary_large_image")` in its `meta`.

An image covers every route under its directory. A nearer one replaces it. The file wins over an `og("image", ...)` a `meta` returns, as it does in Next. A crawler reads the URL as absolute only, so the host writes it against `document.origin`; set that, or the tag carries a path the card ignores. PNG, JPEG, GIF and WebP are accepted, since those are what the crawlers decode. The build refuses a directory with two pictures of one kind and a picture with no route under it.

## A CDN in front

A deployment whose static tree a CDN serves sets the base at build time, since the URLs are written into the markup:

```toml
[images]
base = "https://cdn.example.com"

[fonts]
base = "https://cdn.example.com"
```

The host widens the policy with the origin. The CDN has to send `Access-Control-Allow-Origin` for the fonts, since the browser fetches a font across origins in CORS mode whatever the page says. `fsr bundle` writes the files under `serve/`; getting them onto the CDN is the deployment's job.

## Where things live

Every directory is a convention, moved with one table:

```toml
[dirs]
styles = "assets/css"
fonts = "assets/fonts"
images = "assets/images"
icons = "assets/icons"
```

The routes stay what they are. `styles`, `fonts` and `icons` are scanned; `images` is the one directory a template names a file under, since a component finds its images by import wherever they sit.

## The lab

Every path this chapter names is on one of the two pages of [gallery_react_ts](../../examples/gallery_react_ts/README.md), each row of its README naming the path it exercises; `cargo test -p gallery_react_ts` asserts the markup for each. The storefront below is the production-shaped one.

Run the storefront and open the catalog with the network panel on. Fourteen cards fetch fourteen AVIF files at 640 wide, 31 KB in all, against 200 KB for the originals. The first card's file was requested by the preload before the stylesheet. Narrow the window to a phone and reload: the same cards fetch the 960 variants, since the slot is full width on a two-pixel-per-CSS-pixel screen. Block `*.woff2` in the panel and reload: every heading and price sits where it did with Inter, since the fallback took the same room. Then change `widths` in `config/app.toml` to `[320]`, save and watch the report say `derived 28 files under dist/` once and `0` on the save after.
