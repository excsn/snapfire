# Packages the portal vendors

Two web module packages whose components are styled with Tailwind classes the portal's stylesheet builds. Each is built with snapfirec and its output copied into `app/vendor/@acme/`.

| Package | Shows | Built with |
| --- | --- | --- |
| `acme_ui` | A class list the package's build writes, read in place of its modules, a lookup table's classes included | `--emit-tailwind-classes` |
| `acme_charts` | A package with no class list, read from its minified module the way a third-party bundle is | no class list |

```sh
snapfirec --root packages/acme_ui --minify compact --declaration --emit-tailwind-classes
snapfirec --root packages/acme_charts --minify compact --declaration
cp packages/acme_ui/dist/{index.min.js,cn.min.js,index.d.ts,cn.d.ts,tailwind.classes} app/vendor/@acme/ui/
cp packages/acme_charts/dist/{index.min.js,index.d.ts} app/vendor/@acme/charts/
```

`app/importmap.json` maps `@acme/ui` and `@acme/charts` to the copies and `app/vendor/.fsr-vendor.json` records them. `src/ui/Vendored.tsx` places both on `/themes`.
