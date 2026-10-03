import { CodeRenderable, type MarkdownOptions, SyntaxStyle } from "@opentui/core";

import { eventDiffs } from "@session/diffs.ts";
import { EventKind, type ThreadEvent, eventPresentation, eventText, toolPreview } from "@session/events.ts";
import { BOLD, Tone } from "@theme/themes.ts";
import { useTheme } from "@theme/providers.tsx";
import { DiffRows } from "@widgets/conversation/diffRows.tsx";
import { WrappingText } from "@widgets/conversation/wrappingTexts.tsx";

const syntaxStyle = SyntaxStyle.create();

const wrapMarkdownText: NonNullable<MarkdownOptions["renderNode"]> = (_token, context) => {
  const renderable = context.defaultRender();
  if (renderable instanceof CodeRenderable) renderable.wrapMode = "word";
  return renderable;
};

function isToolEvent(event: ThreadEvent): boolean {
  return event.kind === EventKind.ToolCall || event.kind === EventKind.ToolResult;
}

function toneColor(tone: Tone, theme: ReturnType<typeof useTheme>): string | undefined {
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

export function EventRow(props: { event: ThreadEvent }) {
  const theme = useTheme();
  const isUser = () => props.event.kind === EventKind.UserMessage;
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
    const diffs = () => (props.event.kind === EventKind.ToolCall ? eventDiffs(props.event) : []);
    return (
      <box width="100%" flexDirection="column" marginTop={1}>
        <box width="100%" flexDirection="row" paddingX={1} gap={1}>
          <text width={1} flexShrink={0} fg={glyphColor()} attributes={BOLD}>
            {presentation().glyph}
          </text>
          <WrappingText color={theme.textMuted} content={toolPreview(props.event.content, props.event.kind === EventKind.ToolCall ? "started" : "completed")} />
        </box>
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
