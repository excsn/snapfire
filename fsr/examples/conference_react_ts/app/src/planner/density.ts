import { createContext } from "react";

export type Density = "comfortable" | "compact";

export const DensityContext = createContext<Density>("comfortable");
