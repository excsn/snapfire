import type { Ctx } from "@snapfire/fsr";

export async function load(_ctx: Ctx) {
  return {
    flags: [
      { flag: "--source-map", does: "Writes a .map beside every output." },
      { flag: "--minify compact", does: "Minifies without mangling what a stack trace needs." },
      { flag: "--public-path <prefix>", does: "The URL prefix the browser fetches the outputs under." },
      { flag: "--import-map <file>", does: "Fails the build when a bare import has no entry." },
    ],
  };
}

export const meta = () => ({
  title: "SnapFire Compiler · TypeScript for the browser without Node.js",
  description: "SnapFire Compiler is a native TypeScript and CSS compiler that emits browser-ready ES modules, source maps and a preload manifest, with no Node and no node_modules. Its binary is snapfirec.",
});
