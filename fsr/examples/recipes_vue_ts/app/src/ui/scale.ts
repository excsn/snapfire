import type { InjectionKey } from "vue";

/** How many times the recipe's own serves the scaler is showing, provided by `Scaler.vue` to each `Amount.vue`. */
export const scaleKey: InjectionKey<number> = Symbol("scale");
