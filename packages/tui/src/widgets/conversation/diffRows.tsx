import { For, Show } from "solid-js";

import type { FileDiff } from "@session/diffs.ts";
import { useTheme } from "@theme/providers.tsx";
import { BOLD } from "@theme/themes.ts";

function summary(diff: FileDiff): string {
  const more = diff.truncated > 0 ? `  (${diff.truncated} more lines)` : "";
  return `${diff.path}  +${diff.added} -${diff.removed}${more}`;
}

export function DiffRows(props: { diffs: FileDiff[] }) {
  const theme = useTheme();
  return (
    <For each={props.diffs}>
      {(file) => (
        <box width="100%" flexDirection="column" paddingLeft={3} paddingRight={1}>
          <text fg={theme.textMuted} attributes={BOLD} wrapMode="word">
            {summary(file)}
          </text>
          <Show when={file.diff}>
            <diff
              width="100%"
              diff={file.diff}
              view="unified"
              showLineNumbers={false}
              wrapMode="word"
              addedSignColor={theme.success ?? theme.accent}
              removedSignColor={theme.err}
              fg={theme.text}
            />
          </Show>
        </box>
      )}
    </For>
  );
}
