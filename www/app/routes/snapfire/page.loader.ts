import type { Ctx } from "@snapfire/fsr";

export async function load(_ctx: Ctx) {
  return { version: "0.5.0" };
}

export const meta = () => ({
  title: "snapfire · Tera templates over Actix with live reload",
  description: "A Rust templating library with first-class Tera 2 and Actix Web integration and an integrated zero-configuration live-reload server.",
});
