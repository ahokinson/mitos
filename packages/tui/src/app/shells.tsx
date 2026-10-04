import { RGBA } from "@opentui/core";
import {
  type Accessor,
  Show,
  createMemo,
  createSignal,
  onCleanup,
  onMount,
} from "solid-js";

import { useAppShortcuts } from "@app/shortcuts.ts";
import type { CommandContext, PendingDelete } from "@commands/contexts.ts";
import { dispatchCommand } from "@commands/dispatchers.ts";
import { describeFailure } from "@commands/failures.ts";
import { ParsedKind, parseCommandInput } from "@commands/parsers.ts";
import { type ConfigHandle } from "@config/loaders.ts";
import { resolveDbPath } from "@config/paths.ts";
import {
  answerRequest,
  archiveThread,
  compactThread,
  createThread,
  deleteThread,
  hooksInit,
  hooksStatus,
  noteThread,
  reassignHarness,
  sendMessage,
  setMode,
} from "@session/mutations.ts";
import { LayoutRenderer } from "@layout/renderers.tsx";
import { DEFAULT_LAYOUT_CONFIG, collectFocusableIds } from "@layout/trees.ts";
import { createThreadEventsState } from "@session/events.ts";
import { pruneEmptyThreads } from "@session/exits.ts";
import { FeedbackTone, createFeedbackState } from "@commands/feedbackLines.ts";
import { type KnownHarness, detectInstalledHarnesses } from "@harness/harnesses.ts";
import { type Thread, createThreadListState } from "@session/threads.ts";
import { createSelectionUsageState } from "@session/usage.ts";
import { AppStateProvider } from "@app/states.tsx";
import { createSuggestionsState } from "@commands/suggestionStates.ts";
import { listUserMessages } from "@database/views.ts";
import { watchDatabase } from "@database/watchers.ts";
import { createFocusRing } from "@input/focusRings.ts";
import { resolveKeyBindings } from "@input/keybindings.ts";
import { useTheme } from "@theme/providers.tsx";

const HISTORY_LIMIT = 500;

export function App(props: { configHandle: ConfigHandle }) {
  const theme = useTheme();
  const layout = createMemo(() => props.configHandle.config().layout ?? DEFAULT_LAYOUT_CONFIG);
  const keyBindings = createMemo(() => resolveKeyBindings(props.configHandle.config().keybindings));
  const focusRing = createFocusRing(() => collectFocusableIds(layout()));
  const list = createThreadListState(process.env.MITOS_WORKSPACE_KEY);
  const [selected, setSelected] = createSignal<Thread | null>(null);
  const events = createThreadEventsState(() => selected()?.id ?? null);
  const feedback = createFeedbackState();

  // With no explicit choice, fall back to whatever is installed so a fresh
  // session can send a message without running /new first.
  function resolveDefaultHarness(): string | null {
    return props.configHandle.config().session?.defaultHarness ?? detectInstalledHarnesses()[0] ?? null;
  }
  const [currentHarness, setCurrentHarness] = createSignal<string | null>(resolveDefaultHarness());
  const [pendingDelete, setPendingDelete] = createSignal<PendingDelete | null>(null);
  const [error, setError] = createSignal<string | null>(null);
  const [working, setWorking] = createSignal(false);
  const { usage, requests, bump: bumpUsage } = createSelectionUsageState(selected);

  const suggestions = createSuggestionsState({
    threads: () => list.threads(),
    pendingDeleteId: () => pendingDelete()?.threadId ?? null,
  });

  function refresh(): void {
    list.refresh();
    const id = selected()?.id;
    if (id) setSelected(list.threads().find((thread) => thread.id === id) ?? null);
    events.sync();
    bumpUsage();
  }

  /** Threads this session created on launch; any still empty at exit are deleted. */
  const launched = new Set<string>();

  /** Launch always starts a fresh thread; `/resume` and `/threads` reach old ones. */
  async function initializeSelection(): Promise<void> {
    try {
      const harness = resolveDefaultHarness();
      const thread = await createThread(harness);
      launched.add(thread.id);
      setSelected(thread);
      setCurrentHarness(harness);
      list.refresh();
    } catch (cause) {
      setError(describeFailure(cause));
    }
  }

  let quitting = false;
  async function quit(): Promise<void> {
    if (quitting) return;
    quitting = true;
    await pruneEmptyThreads(launched);
    process.exit(0);
  }

  onMount(() => {
    refresh();
    void initializeSelection();
    onCleanup(watchDatabase(resolveDbPath(process.env.MITOS_STATE_DIR), refresh));
    for (const signal of ["SIGTERM", "SIGHUP"] as const) process.on(signal, () => void quit());
  });

  async function withWorking(run: () => Promise<void>): Promise<void> {
    setWorking(true);
    try {
      setError(null);
      await run();
    } catch (cause) {
      setError(describeFailure(cause));
    } finally {
      setWorking(false);
    }
  }

  const createAndSend = (harness: string, message: string) =>
    withWorking(async () => {
      const thread = await createThread(harness);
      setSelected(thread);
      setCurrentHarness(harness);
      list.refresh();
      await sendMessage(thread.id, message);
      events.sync();
      bumpUsage();
    });

  const sendToThread = (thread: Thread, message: string) =>
    withWorking(async () => {
      await sendMessage(thread.id, message);
      events.sync();
      bumpUsage();
    });

  function startNewThread(harness: string | null): void {
    setSelected(null);
    setCurrentHarness(harness);
    setError(null);
    feedback.push({
      threadId: null,
      tone: FeedbackTone.Info,
      lines: harness
        ? `New thread draft started — next message creates a ${harness} thread.`
        : "New thread draft started — no harness chosen. Run /new <harness> or send a message once one is set.",
    });
  }

  function buildCommandContext(): CommandContext {
    return {
      list,
      selected,
      setSelected,
      setCurrentHarness,
      defaultHarness: resolveDefaultHarness,
      startNewThread,
      events,
      pendingDelete,
      setPendingDelete,
      pendingRequests: requests,
      refresh,
      bumpUsage,
      note: (threadId, tone, lines) => feedback.push({ threadId, tone, lines }),
      mutations: { reassignHarness, archiveThread, compactThread, deleteThread, noteThread, setMode, answerRequest, hooksStatus, hooksInit },
    };
  }

  function handleSend(message: string): void {
    const text = message.trim();
    if (!text) return;
    const parsed = parseCommandInput(text);
    if (parsed.kind === ParsedKind.Command) {
      void dispatchCommand(parsed.command, buildCommandContext());
      return;
    }
    const thread = selected();
    if (thread) {
      void sendToThread(thread, parsed.text);
      return;
    }
    const harness = currentHarness();
    if (harness && detectInstalledHarnesses().includes(harness as KnownHarness)) {
      void createAndSend(harness, parsed.text);
      return;
    }
    feedback.push({
      threadId: null,
      tone: FeedbackTone.Error,
      lines: 'No harness selected. Run "/new <harness>" (e.g. "/new codex") to start a thread, or /help for commands.',
    });
  }

  useAppShortcuts({
    suggestions,
    keyBindings,
    layout,
    focusRing,
    startNewThread: () => startNewThread(resolveDefaultHarness()),
    refresh,
    quit: () => void quit(),
  });

  return (
    <AppStateProvider
      value={{
        list,
        selected,
        currentHarness,
        events,
        feedback,
        onSend: handleSend,
        loadHistory: () => listUserMessages(process.env.MITOS_WORKSPACE_KEY, HISTORY_LIMIT),
        onDraftChange: suggestions.setDraft,
        registerSuggestionApplier: suggestions.registerApplier,
        suggestions: suggestions.suggestions,
        suggestionCursor: suggestions.cursor,
        working,
        pendingRequests: requests,
        usage,
        focusRing,
      }}
    >
      <box flexDirection="column" width="100%" height="100%" backgroundColor={RGBA.defaultBackground()}>
        <Show when={error()}>{(message: Accessor<string>) => <text fg={theme.err}>{message()}</text>}</Show>
        <LayoutRenderer config={layout()} />
      </box>
    </AppStateProvider>
  );
}
