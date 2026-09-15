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

The build reads the import and stops there: a file in a language it does not read is a component the server has no body for. It is placed as an island, its props are lowered like any other placement and the server writes an empty `<sf-i>` with the props script beside it. The browser sees no server markup and mounts rather than hydrates. Place the same component outside `Island` and the build refuses, naming the tag: a component the server cannot render can only be an island.

`Island`, `island`, `Link` and `Slot` come from `@snapfire/fsr-authoring/template` here rather than from `@snapfire/fsr-client/react`. They are the same placements and the build reads either import; the difference is what types them. The React module's declarations import React's, so a React application uses it. The template module's carry no React, so an application without React uses it. `Children` is what such a layout writes where a React one writes `ReactNode`.

## The component itself

`Tonight.vue` is an ordinary single-file component: a `<script setup lang="ts">`, a `<template>` and a `<style scoped>`.

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

`useStore` from `@snapfire/fsr-client/vue` is the store as a Vue ref: the same keyed store the layout's loader seeds and a React island reads with its own `useStore`, so a Vue masthead and a React panel would show the same number. The plan control in the example calls `actions.recipe.$id.plan(...)` from the generated client and writes the store optimistically, the same way the storefront's React button does.

## What the build does with it

`snapfirec` does not compile Vue. When it meets a `.vue` file it looks for `snapfirec-vue` on `PATH` and hands every `.vue` file of the project to that one process in a batch, before anything else is planned. The plugin carries Vue's own compiler, run in QuickJS, so no Node is involved; `cargo install snapfire_vue` is the whole install. Without it the build stops:

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

Run `fsr build app` in the recipes example and read `app/generated/islands.ts`: three registrations, all `.vue`, one mounter import. Read the report: every route module is `static`. View the source of a recipe page before the scripts run: the plan control is an empty `<sf-i>` followed by its props. The three component stylesheets are linked in the head after `box.css`.

Take `snapfirec-vue` off `PATH` and build again. The error names the binary, the install command and the three files.

Put an unclosed tag in `Scaler.vue` and build. The plugin's diagnostic names the file and the line, the other two components compile and the build stops.

Give `routes/page.tsx` a `useState`. Build: the page stops being `static`, it appears in the registry with the React mounter and the bundle asks the import map for `react/jsx-runtime`, which this application does not have. That failure is the whole reason the static rule exists. Take it back out.

Open the masthead panel in a browser, then click "Cook this tonight" on a recipe. The count moves, the button changes and the panel is still open: the layout re-rendered around its island and the island kept its state.
