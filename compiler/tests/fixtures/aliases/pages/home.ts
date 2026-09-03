import { greet } from "@src/util/greet";
import { lib } from "@lib";
import { sibling } from "@src/lib/sibling";
import { format } from "date-fns";

export const page = greet(lib) + sibling + format();
