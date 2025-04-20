import { Text } from "ink";
import React from "react";

export interface TerminalInlineImageProps {
  src: string | Buffer | Uint8Array;
  alt?: string;
  width?: number | string;
  height?: number | string;
}

// During tests or when terminal does not support images, fallback to alt.
export default function TerminalInlineImage({ alt = "[image]" }: TerminalInlineImageProps): React.ReactElement {
  return <Text>{alt}</Text>;
}
