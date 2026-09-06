import type { Ctx } from "@snapfire/fsr";

export async function load(_ctx: Ctx) {
  return {
    benchmarks: [
      { name: "Product Page", cold: "19.7 ms", warm: "471 µs", fsr: "129 µs", speedup: "3.7x" },
      { name: "Cart (3 items)", cold: "19.6 ms", warm: "505 µs", fsr: "128 µs", speedup: "3.9x" },
      { name: "Catalog (12 cards)", cold: "20.0 ms", warm: "1.77 ms", fsr: "999 µs", speedup: "1.8x" },
    ],
  };
}

export const meta = () => ({
  title: "Benchmarks · SnapFire FSR",
  description: "Criterion benchmarks comparing FSR native Rust IR rendering against QuickJS React 18 SSR.",
});