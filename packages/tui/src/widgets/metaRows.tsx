import { useTheme } from "@theme/providers.tsx"

/** `stacked` puts the value on its own line, for values too wide to sit
 * beside their label in a narrow pane (timestamps, paths). */
export function MetaRow(props: { label: string; value: string; valueColor?: string; stacked?: boolean }) {
  const theme = useTheme()
  if (props.stacked) {
    return (
      <box width="100%" flexDirection="column">
        <text fg={theme.textMuted}>{props.label}</text>
        <text fg={props.valueColor ?? theme.text}>{props.value}</text>
      </box>
    )
  }
  return (
    <box width="100%" flexDirection="row" justifyContent="space-between" gap={2}>
      <text flexShrink={0} fg={theme.textMuted}>
        {props.label}
      </text>
      <text flexGrow={1} flexShrink={1} flexBasis={0} minWidth={0} wrapMode="word" fg={props.valueColor ?? theme.text}>
        {props.value}
      </text>
    </box>
  )
}
