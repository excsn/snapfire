// The props of ink-big-text 2.0.0, https://github.com/sindresorhus/ink-big-text, with `font` the font file's JSON rather than a name: FSR ships no cfonts font.

import type { ReactElement } from "react";
import { Text, useWindowSize } from "@snapfire/fsr/terminal/ink";
import { bigText } from "@snapfire/fsr/terminal";

type Props = {
  text: string;
  font: unknown;
  align?: "left" | "center" | "right";
  colors?: string[];
  backgroundColor?: string;
  letterSpacing?: number;
  lineHeight?: number;
  space?: boolean;
  maxLength?: number;
};

function BigText({ text, font, align = "left", colors = ["system"], backgroundColor = "transparent", letterSpacing = 1, lineHeight = 1, space = true, maxLength = 0 }: Props): ReactElement {
  const { columns } = useWindowSize();
  return <Text>{bigText(font, text, { align, colors, background: backgroundColor, letterSpacing, lineHeight, space, maxLength, width: columns })}</Text>;
}

export default BigText;
