import type { ColorInput } from "@opentui/core";

export function WrappingText(props: { color: ColorInput | undefined; content: string }) {
  return (
    <text flexGrow={1} flexShrink={1} flexBasis={0} minWidth={0} wrapMode="word" fg={props.color}>
      {props.content}
    </text>
  );
}
