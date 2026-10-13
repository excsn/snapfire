# 306. What a page asks other origins for

The question this chapter answers: when a visitor opens one of your pages, which requests leave for hosts other than yours? How do you keep one of them on your own origin when privacy or offline use calls for it?

**For:** app developers, and whoever answers for a site's privacy notice.

## The defaults

A page FSR renders asks your origin for its document, its modules, its stylesheets, its images and its fonts. Something else is reached only when the application names it. Each one shows in the boot report's policy rows, since the host adds every other origin a page reaches to the Content-Security-Policy it sends.

| What | Reaches | When | Keep it on your origin |
| --- | --- | --- | --- |
| A script from `scriptTag` or `loadScript` | the script's host | on load, when idle or on the first interaction, as written | serve the file from a `[[static]]` root |
| An `<Embed>`'s still | the provider's image CDN, `i.ytimg.com` or Vimeo's | on load | `[media] embed_stills = "local"` |
| An `<Embed>`'s player | `www.youtube-nocookie.com` or `player.vimeo.com` | only when the visitor plays it | nothing: the video lives there |
| A `<video>`, `<audio>`, `<source>` or `<track>` with a URL on another host | that host | as the element's `preload` says | import the file, which serves it from your origin |
| An image through a named `[images] sources` entry | the image service | on load | import the file instead |
| A font with `[fonts.<key>] remote` | the provider's stylesheet and files | on load | `fsr fonts add`, which downloads the files under `fonts/` |
| `[images] base` and `[fonts] base` | the CDN named | on load | leave them unset |

Nothing in this list happens unless the application wrote it. A card image, a sitemap, a robots file and the client modules are always served from your origin.

## Video embeds

`<Embed provider="youtube" id="…" title="…" />` renders a still with a play button. The player's iframe loads only when the visitor clicks, which is why a page with three embeds costs three images rather than three players:

```tsx
import { Embed } from "@snapfire/fsr-client/react";

<Embed provider="youtube" id="f_WuRfuMXQw" title="Rick Astley - Never Gonna Give You Up" />
```

By default the still is the provider's own image, so the page asks the provider's image CDN for it on load. YouTube's still is named by the id. Vimeo's comes from Vimeo's public video API, which the build asks once and keeps the answer under `generated/embeds/`.

To keep the provider out of the page until the visitor plays, fetch the stills at build:

```toml
[media]
embed_stills = "local"
```

The build then fetches each still once into `generated/embeds/` and serves it from your origin under its content hash. The page asks the provider for nothing until a click and shows its stills offline, as a service worker caches them with the rest of the site. A fetch that fails, a build with no network for example, falls back to the provider's image and says so in the build output. The player always comes from the provider, since that is where the video is.

The player loads from `youtube-nocookie.com`, YouTube's domain for embeds that sets no cookie until the video plays.

## Content-Security-Policy

With a `[document.csp]` the host widens each directive by the origins above that the build can read: `script-src` for a script's host, `img-src` for a still or any image with a literal URL, `frame-src` for a player and `media-src` for a media element's URL. A URL computed at request time is one the build cannot read, so its origin goes in the policy by hand; for a script the boot report names the row that computes it. Chapter 305 has the policy for scripts in full.

## The lab

The conference app's venue page has two embeds, Rick Astley and the Gummibär song. Open `/venue` and read the browser's network log: two images from `i.ytimg.com` and nothing from `youtube-nocookie.com`. Click the first and its player loads, while the second stays a still. Set `[media] embed_stills = "local"`, build and load the page again: the stills come from `/static/js/app/generated/embeds/` and the log shows no request to YouTube until the click.
