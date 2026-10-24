import type { ReactElement } from "react";
import type { SpinnerName } from "@snapfire/fsr/terminal/spinners";
type Props = {
	type?: SpinnerName;
};
declare function Spinner({ type }: Props): ReactElement;
export default Spinner;
