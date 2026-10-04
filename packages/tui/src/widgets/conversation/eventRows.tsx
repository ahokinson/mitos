import { CodeRenderable, type ColorInput, type MarkdownOptions } from "@opentui/core";
import { For, Show } from "solid-js";

import { eventDiffs } from "@session/diffs.ts";
import { EventKind, type ThreadEvent, eventPresentation, eventText } from "@session/events.ts";
import { ToolKind, isQuietResult, toolCall, toolGlyph, toolResult, toolTone } from "@session/tools.ts";
import { BOLD, Tone } from "@theme/themes.ts";
import { useSyntaxStyle, useTheme } from "@theme/providers.tsx";
import { DiffRows } from "@widgets/conversation/diffRows.tsx";
import { WrappingText } from "@widgets/conversation/wrappingTexts.tsx";

const wrapMarkdownText: NonNullable<MarkdownOptions["renderNode"]> = (_token, context) => {
  const renderable = context.defaultRender();
  if (renderable instanceof CodeRenderable) renderable.wrapMode = "word";
  return renderable;
};

export function isToolEvent(event: ThreadEvent): boolean {
  return event.kind === EventKind.ToolCall || event.kind === EventKind.ToolResult;
}

function isClaudeResult(event: ThreadEvent): boolean {
  const payload = event.payload as { type?: unknown } | null | undefined;
  return event.kind === EventKind.ToolResult && payload?.type === "tool_result";
}

function toneColor(tone: Tone, theme: ReturnType<typeof useTheme>): ColorInput | undefined {
  switch (tone) {
    case Tone.Accent:
      return theme.accent;
    case Tone.Success:
      return theme.success ?? theme.accent;
    case Tone.Warning:
      return theme.warning;
    case Tone.Error:
      return theme.err;
    case Tone.Muted:
      return theme.textMuted;
    case Tone.Dim:
      return theme.textDim;
  }
}

function ToolCallLine(props: { event: ThreadEvent }) {
  const theme = useTheme();
  const call = () => toolCall(props.event);
  const head = () => call().lines[0] ?? "";
  const named = () => call().kind === ToolKind.Other || head() === "";
  return (
    <box width="100%" flexDirection="row" paddingX={1} gap={1}>
      <text width={1} flexShrink={0} fg={toneColor(toolTone(call().kind), theme)} attributes={BOLD}>
        {toolGlyph(call().kind)}
      </text>
      <box flexGrow={1} flexShrink={1} flexBasis={0} minWidth={0} flexDirection="column">
        <text wrapMode="word" fg={theme.text}>
          <Show when={named() && call().name}>
            <b>{call().name}</b>
            {head() ? "  " : ""}
          </Show>
          <span style={{ fg: theme.textMuted }}>{head()}</span>
        </text>
        <For each={call().lines.slice(1)}>
          {(line) => (
            <text wrapMode="word" fg={theme.textMuted}>
              {line}
            </text>
          )}
        </For>
        <Show when={call().hidden > 0}>
          <text fg={theme.textDim}>… +{call().hidden} more {call().hidden === 1 ? "line" : "lines"}</text>
        </Show>
      </box>
    </box>
  );
}

/** `grouped`: the row directly follows another tool row, so it sits flush
 * against it instead of starting a new block. `folded`: a call whose result
 * diff already names the file, so the call row is not drawn. */
export function EventRow(props: { event: ThreadEvent; grouped?: boolean; folded?: boolean }) {
  const theme = useTheme();
  const syntaxStyle = useSyntaxStyle();
  const isUser =() => props.event.kind === EventKind.UserMessage;
  const isAssistant = () => props.event.kind === EventKind.AssistantMessage || props.event.kind === EventKind.AssistantDelta;
  const presentation = () => eventPresentation(props.event.kind);
  const glyphColor = () => toneColor(presentation().tone, theme);

  if (isUser()) {
    return (
      <box
        width="100%"
        flexDirection="row"
        marginTop={1}
        paddingX={1}
        paddingRight={3}
        gap={1}
        backgroundColor={theme.backgroundSelection}
      >
        <text width={1} flexShrink={0} fg={glyphColor()} attributes={BOLD}>
          {presentation().glyph}
        </text>
        <WrappingText color={theme.text} content={props.event.content ?? ""} />
      </box>
    );
  }

  if (isAssistant()) {
    return (
      <box width="100%" flexDirection="row" marginTop={1} paddingX={1} paddingRight={3} gap={1}>
        <text width={1} flexShrink={0} fg={glyphColor()} attributes={BOLD}>
          {presentation().glyph}
        </text>
        <markdown flexGrow={1} flexShrink={1} flexBasis={0} minWidth={0} content={props.event.content ?? ""} syntaxStyle={syntaxStyle} renderNode={wrapMarkdownText} />
      </box>
    );
  }

  if (isToolEvent(props.event)) {
    if (props.folded) return null;
    const diffs = () => (props.event.kind === EventKind.ToolCall || isClaudeResult(props.event) ? eventDiffs(props.event) : []);
    const isResult = () => props.event.kind === EventKind.ToolResult;
    const quiet = () => isQuietResult(props.event);
    const result = () => toolResult(props.event.content);
    const startsBlock = () => !isResult() || diffs().length > 0;
    return (
      <box width="100%" flexDirection="column" marginTop={startsBlock() && !props.grouped ? 1 : 0}>
        <Show when={isResult()} fallback={<Show when={!props.folded}><ToolCallLine event={props.event} /></Show>}>
          <Show when={!quiet()}>
            <box width="100%" flexDirection="row" paddingX={1} gap={1}>
              <text width={1} flexShrink={0} fg={glyphColor()} attributes={BOLD}>
                {presentation().glyph}
              </text>
              <box flexGrow={1} flexShrink={1} flexBasis={0} minWidth={0} flexDirection="column">
                <For each={result().lines.length > 0 ? result().lines : ["completed"]}>
                  {(line) => (
                    <text wrapMode="word" fg={theme.textDim}>
                      {line}
                    </text>
                  )}
                </For>
                <Show when={result().hidden > 0}>
                  <text fg={theme.textDim}>… +{result().hidden} more {result().hidden === 1 ? "line" : "lines"}</text>
                </Show>
              </box>
            </box>
          </Show>
        </Show>
        <DiffRows diffs={diffs()} />
      </box>
    );
  }

  return (
    <box width="100%" flexDirection="row" marginTop={1} paddingX={1} gap={1}>
      <text width={1} flexShrink={0} fg={glyphColor()} attributes={BOLD}>
        {presentation().glyph}
      </text>
      <WrappingText color={props.event.kind === EventKind.Error ? theme.err : theme.textMuted} content={eventText(props.event)} />
    </box>
  );
}
