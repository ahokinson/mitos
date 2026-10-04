import { pathToFiletype } from "@opentui/core";
import { For, Show } from "solid-js";

import type { FileDiff } from "@session/diffs.ts";
import { ToolKind, shortenPath, toolGlyph } from "@session/tools.ts";
import { blend } from "@theme/colors.ts";
import { useSyntaxStyle, useTheme, useThemeTokens } from "@theme/providers.tsx";
import { BOLD } from "@theme/themes.ts";

const LINE_TINT = 0.18;
const GUTTER_TINT = 0.28;

function DiffHeader(props: { diff: FileDiff }) {
  const theme = useTheme();
  return (
    <box width="100%" flexDirection="row" paddingX={1} gap={1}>
      <text width={1} flexShrink={0} fg={theme.success ?? theme.accent} attributes={BOLD}>
        {toolGlyph(ToolKind.Edit)}
      </text>
      <text flexGrow={1} flexShrink={1} flexBasis={0} minWidth={0} fg={theme.textMuted} wrapMode="word">
        {shortenPath(props.diff.path)}
        {"  "}
        <span style={{ fg: theme.success ?? theme.accent }}>+{props.diff.added}</span>{" "}
        <span style={{ fg: theme.err }}>-{props.diff.removed}</span>
        {props.diff.truncated > 0 ? `  (${props.diff.truncated} more lines)` : ""}
      </text>
    </box>
  );
}

export function DiffRows(props: { diffs: FileDiff[] }) {
  const theme = useTheme();
  const tokens = useThemeTokens();
  const syntaxStyle = useSyntaxStyle();
  const base = tokens.backgroundChrome;
  const tint = (token: string | undefined, alpha: number) =>
    base && token ? blend(token, base, alpha) : undefined;
  const added = tokens.success ?? tokens.accent;
  const solid = base === undefined;
  return (
    <For each={props.diffs}>
      {(file) => (
        <box width="100%" flexDirection="column">
          <DiffHeader diff={file} />
          <Show when={file.diff}>
            <box width="100%" paddingLeft={3} paddingRight={1}>
              <diff
                width="100%"
                diff={file.diff}
                view="unified"
                filetype={pathToFiletype(file.path)}
                syntaxStyle={syntaxStyle}
                showLineNumbers
                lineNumberFg={solid ? (theme.diffText ?? theme.text) : theme.textDim}
                lineNumberBg={solid ? theme.textDim : base}
                addedLineNumberBg={solid ? (theme.success ?? theme.accent) : tint(added, GUTTER_TINT)}
                removedLineNumberBg={solid ? theme.err : tint(tokens.err, GUTTER_TINT)}
                wrapMode="word"
                addedBg={solid ? (theme.success ?? theme.accent) : tint(added, LINE_TINT)}
                removedBg={solid ? theme.err : tint(tokens.err, LINE_TINT)}
                contextBg={solid ? theme.textDim : base}
                addedSignColor={theme.success ?? theme.accent}
                removedSignColor={theme.err}
                fg={solid ? (theme.diffText ?? theme.text) : theme.text}
              />
            </box>
          </Show>
        </box>
      )}
    </For>
  );
}
