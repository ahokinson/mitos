import type { TextareaRenderable } from "@opentui/core";
import { useKeyboard } from "@opentui/solid";
import { createSignal } from "solid-js";

import { ThreadMode } from "@session/threads.ts";
import { BOLD, Chrome } from "@theme/themes.ts";
import { useTheme } from "@theme/providers.tsx";
import { createInputHistory } from "@widgets/conversation/histories.ts";

const MAX_COMPOSER_LINES = 4;

export function Composer(props: {
  focused: boolean;
  hasSession: boolean;
  working: boolean;
  mode: ThreadMode;
  loadHistory: () => readonly string[];
  onSubmit: (text: string) => void;
  onDraftChange: (text: string) => void;
  registerSuggestionApplier: (apply: (text: string) => void) => void;
}) {
  const theme = useTheme();
  const [height, setHeight] = createSignal(1);
  const history = createInputHistory(() => props.loadHistory());
  let editor: TextareaRenderable | undefined;

  function setDraft(text: string): void {
    editor?.setText(text);
    if (editor) editor.cursorOffset = text.length;
    resizeToContent();
    props.onDraftChange(text);
  }

  useKeyboard((event) => {
    if (!props.focused || !editor || event.defaultPrevented) return;
    if (event.name !== "up" && event.name !== "down") return;
    const text = event.name === "up" ? history.prev(editor.plainText) : history.next(editor.plainText);
    if (text === undefined) return;
    event.preventDefault();
    setDraft(text);
  });

  function resizeToContent(): void {
    const lines = editor?.editorView.getTotalVirtualLineCount() ?? 1;
    setHeight(Math.min(Math.max(lines, 1), MAX_COMPOSER_LINES));
  }

  function handleContentChange(): void {
    resizeToContent();
    props.onDraftChange(editor?.plainText ?? "");
  }

  function submit(): void {
    if (!editor) return;
    const text = editor.plainText.trim();
    if (!text) return;
    // Commands (/approve, /answer, /mode) must work mid-turn; chat text must not.
    if (props.working && !text.startsWith("/")) return;
    history.reset();
    editor.setText("");
    setHeight(1);
    props.onDraftChange("");
    props.onSubmit(text);
  }

  return (
    <box
      width="100%"
      flexShrink={0}
      flexDirection="row"
      alignItems="flex-start"
      gap={1}
      paddingX={1}
      marginTop={1}
      border={theme.chrome === Chrome.Framed}
      borderColor={theme.textDim}
      focusedBorderColor={theme.accent}
      backgroundColor={props.focused ? theme.backgroundSelection : theme.backgroundChrome}
    >
      <text fg={props.mode === ThreadMode.Plan ? theme.warning : theme.accent} attributes={BOLD}>
        {props.mode === ThreadMode.Plan ? "plan ›" : "›"}
      </text>
      <textarea
        ref={(element) => {
          editor = element;
          props.registerSuggestionApplier(setDraft);
        }}
        flexGrow={1}
        flexShrink={1}
        flexBasis={0}
        minWidth={0}
        height={height()}
        maxHeight={MAX_COMPOSER_LINES}
        focused={props.focused}
        placeholder={props.hasSession ? "Message…" : "Start a conversation…"}
        placeholderColor={theme.textDim}
        backgroundColor="transparent"
        focusedBackgroundColor="transparent"
        textColor={theme.text}
        focusedTextColor={theme.text}
        cursorColor={theme.accent}
        selectionBg={theme.backgroundSelection}
        selectionFg={theme.text}
        wrapMode="word"
        keyBindings={[
          { name: "return", action: "submit" },
          { name: "return", shift: true, action: "newline" },
        ]}
        onContentChange={handleContentChange}
        onSubmit={submit}
      />
    </box>
  );
}
