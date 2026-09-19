{{htmx_import}}import { boot, enableNavigation } from "@snapfire/fsr-client";
{{htmx_bind_import}}import { registerIslands } from "@generated/islands.js";

registerIslands();

boot();
enableNavigation();
{{htmx_bind}}