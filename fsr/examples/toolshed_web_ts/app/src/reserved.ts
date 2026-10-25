import { createContext } from "@snapfire/fsr/template";

/** The ids of the tools this session has reserved, which the shelf provides to each of its rows. */
export const Reserved = createContext<string[]>([]);
