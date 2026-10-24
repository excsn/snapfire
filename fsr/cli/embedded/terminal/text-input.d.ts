import type { ReactElement } from "react";
export type Props = {
	readonly placeholder?: string;
	readonly focus?: boolean;
	readonly mask?: string;
	readonly showCursor?: boolean;
	readonly highlightPastedText?: boolean;
	readonly value: string;
	readonly onChange: (value: string) => void;
	readonly onSubmit?: (value: string) => void;
};
declare function TextInput({ value: originalValue, placeholder, focus, mask, highlightPastedText, showCursor, onChange, onSubmit }: Props): ReactElement;
export default TextInput;
type UncontrolledProps = {
	readonly initialValue?: string;
} & Omit<Props, "value" | "onChange">;
export declare function UncontrolledTextInput({ initialValue, ...props }: UncontrolledProps): ReactElement;
