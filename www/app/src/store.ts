import { key } from "@snapfire/fsr-client/store";

export const theme = key<"dark" | "light">("site/theme");
export const requestTime = key<number>("site/render_time");