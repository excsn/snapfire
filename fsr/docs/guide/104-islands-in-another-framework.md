# 104. Islands in another framework

The question this chapter answers: how does a Vue component end up on a page the server rendered, what does the build do with a `.vue` file and what does an application that has no React in it look like?

**For:** app developers.

## A page is a template, an island is a framework

The pages, layouts and boundaries under `routes/` are TSX the build lowers and the server renders; chapter 003 is that story. Nothing in it is React: a template's JSX is a vocabulary the lowerer reads and the server writes the markup. React enters when a component needs the browser, `useState`, a handler, a store read, because then the browser must run it. The build keeps a compiled twin of it for that.

An island written in Vue is the same arrangement with a different runtime. The template places it; the server writes its marker and its props; the browser mounts it with Vue. The recipes example, [`recipes_vue_ts`](../../examples/recipes_vue_ts/README.md), is exactly this: every route is a template, every interactive piece is a `.vue` file under `src/ui/`. There is no React in the import map, the vendor tree, the bundle or the type declarations.

## Placing a `.vue` component

A template imports the component as a file and places it inside `Island`:

```tsx
import { Island, Link, type Children } from "@snapfire/fsr-authoring/template";
import Tonight from "@src/ui/Tonight.vue";

export default function BoxLayout({ children, planned }: { children: Children; planned: number }) {
  return (
    <header className="masthead">
      <nav>
        <Link href="/">Recipes</Link>
      </nav>
      <Island when="load">
        <Tonight count={planned} />
      </Island>
      {children}
    </header>
  );
}
```

The build reads the import and asks `snapfirec-vue` to describe the file: the template as Vue's own parser reads it, the `<script setup>` block and how the template reads each name the script binds. It lowers the component the way it lowers a TSX template, to the same render tree and places it as an island with its props lowered like any other placement. The server writes the component's markup inside the `<sf-i>`, spelled the way Vue's own server renderer spells it, with the props script beside it. The browser hydrates over that markup rather than mounting fresh. Place the same component outside `Island` and the build refuses, naming the tag: a component Vue mounts can only be an island, since Vue's root is the island's.

A component the build cannot read stays foreign: the server writes the `<sf-i>` empty with its props and Vue mounts it fresh, which is what every `.vue` file got before the build read them. The report says which and why, with the line:

```text
rendered  src/ui/Box.vue#default             foreign     src/ui/Box.vue:5:18
foreign   src/ui/Box.vue:5:18                `v-model`
          bind `:value` for the markup and handle the input event in the browser; two-way binding is not lowered
          1 component mounts in the browser for it, written empty by the server
            src/ui/Box.vue#default
```

What lowers is the subset a server can evaluate. In `<script setup>`: `defineProps`, with `withDefaults` around it, `ref`, `shallowRef`, `computed` of an arrow, `reactive`, `useStore` from `@snapfire/fsr-client/vue`, a `const` bound to an expression the build reads and functions, which are the browser's. Lifecycle and watch calls are the browser's too and are passed over. In the template: interpolation, `v-if`, `v-else-if` and `v-else`, `v-for` over a list with an item and an index, a bound attribute, `:class` as a string, an array or an object, `:style`, `v-show`, `v-html`, `v-text` and a plain `<slot />`. Outside that is residue: `v-model`, `v-bind` of a whole object, a named or a scoped slot, a component placed inside the template, `v-slot`, `inject`, a `<script>` without `setup`. The residue names its line in the `.vue` file.

A number deserves a word. An integer a contract types as `bigint` reaches the server as an integer and a Vue component that multiplies it by a literal fails the render, the way chapter 100 says a loader must convert one before arithmetic. The recipe page passes `serves={Number(recipe.serves)}` for that reason; the browser never saw a difference, the server does.

`Island`, `island`, `Link` and `Slot` come from `@snapfire/fsr-authoring/template` here rather than from `@snapfire/fsr-client/react`. They are the same placements and the build reads either import. The template module is the portable form: a file written against it is valid whatever the application serves, typed by the dialect's own declarations when the import map has no React and through React's when it has, where `Children` reads as `ReactNode` and the placements as the React module's. A page on the template module that hydrates loads its placements from the client's `template.js`, which the `react` direction maps beside the React module. The React module is the React-only form, with `useStore`, `useLocale` and `useHoisted` that the template module never promises. A file importing it needs React's declarations to type at all. So a layout like this one writes `Children` and keeps working if the application gains React later; a file that wants React's hooks says so by its import.

## The component itself

`Tonight.vue` is an ordinary single-file component: a `<script setup lang="ts">`, a `<template>` and a `<style scoped>`. Nothing in it is written for the server: the build reads it as it stands.

```vue
<script setup lang="ts">
import { ref } from "vue";
import { useStore } from "@snapfire/fsr-client/vue";
import { plannedCount } from "@src/store";

const props = defineProps<{ count: number }>();
const held = useStore(plannedCount, props.count);
const open = ref(false);
</script>

<template>
  <button class="tonight-count" @click="open = !open">{{ held.value }} for tonight</button>
</template>

<style scoped>
.tonight-count { border-radius: 999px; }
</style>
```

`useStore` from `@snapfire/fsr-client/vue` is the store as a Vue ref: the same keyed store the layout's loader seeds and a React island reads with its own `useStore`, so a Vue masthead and a React panel would show the same number. On the server it reads the store the loader seeded, so the count is in the markup. The plan control in the example calls `actions.recipe.$id.plan(...)` from the generated client and writes the store optimistically, the same way the storefront's React button does.

The server writes what Vue's server renderer would write for the same component and props, anchors included: `<!---->` where the panel's `v-if` rendered nothing, `<!--[-->` and `<!--]-->` around a list or a fragment and the `data-v-` stamp of a scoped style on every element. That is what Vue's client walks when it hydrates. It is checked byte for byte against `@vue/server-renderer` in the lowerer's own tests:

```html
<sf-i id="sf-i0" data-sf-module="src/ui/Tonight.vue#default"><div class="tonight" data-v-8ee200ee><button class="tonight-count" aria-label="tonight" data-v-8ee200ee>0 for tonight</button><!----></div><template data-sf-children>Kept in the session cookie. <a href="/tonight">See them</a>.</template></sf-i>
```

The `<template data-sf-children>` after the markup is the island's children. The panel is closed, so the template placed no `<slot />` this render and the server had nowhere to write them; an inert template after the markup carries them instead, which the parser never shows and the scan never reaches. The mounter reads it before Vue hydrates and takes it out of the document, so the slot has its content the moment the panel opens. When the template does place the slot, the children sit inside it in an `<sf-s data-sf-children>` region, which Vue hydrates as an element it rendered.

## What the build does with it

Neither `fsr` nor `snapfirec` compiles Vue. `fsr build` looks for `snapfirec-vue` on `PATH` and asks it to describe every `.vue` file under the source directories in one batch before any route is lowered; `snapfirec` finds the same binary again when it bundles and hands it the same files to compile. The plugin carries Vue's own compiler, run in QuickJS, so no Node is involved; `cargo install snapfire_vue` is the whole install. Without it `fsr build` leaves every `.vue` component foreign and says so in a `plugins` row; the bundle then stops:

```text
❌ `snapfirec-vue` is not on PATH; `cargo install snapfire_vue` puts it there
   needed by "src/ui/Tonight.vue"
```

What comes back is a module and a stylesheet: `dist/src/ui/Tonight.js`, whose imports the build resolves like any other module's, plus `dist/src/ui/Tonight.vue.css`, the component's `<style scoped>` with its `data-v-` attribute. The template's `import Tonight from "@src/ui/Tonight.vue"` is rewritten to the `.js` beside it. The island registry the build writes registers the module with the Vue mounter:

```ts
import { registerIsland } from "@snapfire/fsr-client";
import { vueMounter, vuePatcher, vueUnmounter } from "@snapfire/fsr-client/vue";

export function registerIslands(): void {
  registerIsland("src/ui/Tonight.vue#default", { loader: () => import("../src/ui/Tonight.vue").then((m) => m.default), mount: vueMounter, patch: vuePatcher, unmount: vueUnmounter });
}
```

A mounter is imported only when a registered module wants it, which is what keeps React off a page that has no React component on it.

The stylesheets reach the head before the first island mounts. The compiler lists them in `dist/.snapfire-build.json` under `styles`; the host reads that file at boot and links each one after the application's own `styles/*.css`, so a component's rules come later in the cascade than the document's.

Unchanged components are not compiled twice. The plugin's output is cached on the worker's own version, the source, the options and every sibling file the component read, in memory across a watch and on disk in the output directory. The build says how much of it the cache answered:

```text
   Plugin:   snapfirec-vue 0.1.0 (@vue/compiler-sfc 3.5.13)
   Plugin cache: 3 of 3 answered
```

A block may point at a sibling, `<style src="./Tonight.css" scoped>`. The plugin asks for the file, the build reads it and sends it. From then on a save to `Tonight.css` recompiles `Tonight.vue`.

## Why the page loads no React

A template with no state and no handlers has nothing for the browser to change. The build marks it `static` in the report: it has no browser twin, it is not in the island registry, it is not compiled and the server writes its markup with no island marker around it. The islands inside it are mounted by the document's own scan. That is what lets the recipes application's bundle be `src/**/*` and two generated files, with the client, its store, its Vue adapter and `vue` as its only externals:

```text
rendered  routes/layout.tsx#default          lowered     static
          routes/page.tsx#default            lowered     static
          routes/recipe/[id]/page.tsx#default lowered     static
```

The rule is the same in a React application. The storefront's catalog page is static too; its layout is not, because it renders the header inline and the header has state. A template hydrates when it has state or handlers of its own or renders a component inline that does; an island's state is the island's.

Navigation still keeps a static layout's DOM. A segment carries a digest of its own markup; a layout whose digest did not move is kept whatever changed below it. When it did move, after an action re-rendered it with a new count, the navigator replaces the layout's markup but keeps every island inside it that the new markup also places, by region key, moving the mounted element into the new markup and handing it its new props. The recipes masthead panel stays open through planning a recipe for that reason.

## The lab

Run `fsr build app` in the recipes example and read `app/generated/islands.ts`: three registrations, all `.vue`, one mounter import. Read the report: every route module is `static` and the three components are `lowered` with `vue` in the detail column. View the source of a recipe page before the scripts run: the plan control is a `<sf-i>` holding the button Vue will hydrate, followed by its props. The three component stylesheets are linked in the head after `box.css`.

Take `snapfirec-vue` off `PATH` and build again. The report says the plugin is not on PATH and the three components mount in the browser instead; the bundle then stops with the binary, the install command and the three files.

Give `Tonight.vue` a `v-model` on an input. The report keeps the page `static`, marks the component `foreign` with the line and the bundle compiles it as before: the panel mounts fresh and everything else on the page is as it was.

Put an unclosed tag in `Scaler.vue` and build. The plugin's diagnostic names the file and the line, the other two components compile and the build stops.

Give `routes/page.tsx` a `useState`. Build: the page stops being `static`, so the registry would mount it through React. This application's import map has no React, so the build stops:

```text
`routes/page.tsx#default` mounts through `@snapfire/fsr-client/react`, but the import map does not name `@snapfire/fsr-client/react`, `react` or `react-dom/client`; `fsr use <app dir> react` writes it
```

That failure is the whole reason the static rule exists. Take it back out.

Open the masthead panel in a browser, then click "Cook this tonight" on a recipe. The count moves, the button changes and the panel is still open: the layout re-rendered around its island and the island kept its state.
