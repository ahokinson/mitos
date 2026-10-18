import { useAppShortcuts } from "@app/shortcuts.ts";
import { AppStateProvider } from "@app/states.tsx";
import type { CommandContext, PendingDelete } from "@commands/contexts.ts";
import { dispatchCommand } from "@commands/dispatchers.ts";
import { describeFailure } from "@commands/failures.ts";
import { createFeedbackState, FeedbackTone } from "@commands/feedbackLines.ts";
import { ParsedKind, parseCommandInput } from "@commands/parsers.ts";
import { createSuggestionsState } from "@commands/suggestionStates.ts";
import type { ConfigHandle } from "@config/loaders.ts";
import { resolveDbPath } from "@config/paths.ts";
import { coreReadsRecent } from "@core/invocations.ts";
import {
  listUserMessages,
  resolveHistoryWatchRoots,
  resolveMiningWatchPaths,
} from "@database/views.ts";
import { watchDatabase, watchHistory } from "@database/watchers.ts";
import {
  detectInstalledHarnesses,
  type KnownHarness,
} from "@harness/harnesses.ts";
import { createFocusRing } from "@input/focusRings.ts";
import { resolveKeyBindings } from "@input/keybindings.ts";
import { DEFAULT_LAYOUT_CONFIG } from "@layout/configs.ts";
import { LayoutRenderer } from "@layout/renderers.tsx";
import { collectFocusableIds } from "@layout/trees.ts";
import { RGBA } from "@opentui/core";
import { createThreadEventsState } from "@session/events.ts";
import { pruneEmptyThreads } from "@session/exits.ts";
import { createMiningState } from "@session/mining.ts";
import {
  abortThread,
  answerRequest,
  archiveThread,
  compactThread,
  createThread,
  deleteThread,
  diffThread,
  forkThread,
  hooksInit,
  hooksStatus,
  listAgents,
  listModels,
  noteThread,
  reassignHarness,
  runCommand,
  sendMessage,
  setAgent,
  setMode,
  setModel,
  shareThread,
} from "@session/mutations.ts";
import { createThreadListState, type Thread } from "@session/threads.ts";
import { createSelectionUsageState } from "@session/usage.ts";
import { useTheme } from "@theme/providers.tsx";
import {
  type Accessor,
  createMemo,
  createSignal,
  onCleanup,
  onMount,
  Show,
} from "solid-js";

const HISTORY_LIMIT = 500;

export function App(props: { configHandle: ConfigHandle }) {
  const theme = useTheme();
  const layout = createMemo(
    () => props.configHandle.config().layout ?? DEFAULT_LAYOUT_CONFIG,
  );
  const keyBindings = createMemo(() =>
    resolveKeyBindings(props.configHandle.config().keybindings),
  );
  const focusRing = createFocusRing(() => collectFocusableIds(layout()));
  const list = createThreadListState(process.env.MITOS_WORKSPACE_KEY);
  const [selected, setSelected] = createSignal<Thread | null>(null);
  const events = createThreadEventsState(() => selected()?.id ?? null);
  const feedback = createFeedbackState();

  // Harness detection hits the filesystem, so it runs at launch and on
  // refresh — never on the keystroke path. Watchers fire refresh whenever
  // the store changes, so a mid-session install is still picked up promptly.
  const [installedHarnesses, setInstalledHarnesses] = createSignal<
    KnownHarness[]
  >(detectInstalledHarnesses());

  // With no explicit choice, fall back to whatever is installed so a fresh
  // session can send a message without running /new first.
  function resolveDefaultHarness(): string | null {
    return (
      props.configHandle.config().session?.defaultHarness ??
      installedHarnesses()[0] ??
      null
    );
  }
  const [currentHarness, setCurrentHarness] = createSignal<string | null>(
    resolveDefaultHarness(),
  );
  const [pendingDelete, setPendingDelete] = createSignal<PendingDelete | null>(
    null,
  );
  const [error, setError] = createSignal<string | null>(null);
  const [working, setWorking] = createSignal(false);
  const {
    usage,
    requests,
    bump: bumpUsage,
  } = createSelectionUsageState(selected);
  const { mining, bump: bumpMining } = createMiningState();

  const suggestions = createSuggestionsState({
    installedHarnesses,
    threads: () => list.threads(),
    pendingDeleteId: () => pendingDelete()?.threadId ?? null,
  });

  function refresh(): void {
    const installed = detectInstalledHarnesses();
    setInstalledHarnesses((current) =>
      current.length === installed.length &&
      current.every((harness, index) => harness === installed[index])
        ? current
        : installed,
    );
    list.refresh();
    const id = selected()?.id;
    if (id)
      setSelected(list.threads().find((thread) => thread.id === id) ?? null);
    events.sync();
    bumpUsage();
    bumpMining();
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
    // The TUI's own reads churn the watched WAL files, so their shadow is
    // skipped; an external write during a shadow is caught by the next one.
    onCleanup(
      watchDatabase(resolveDbPath(process.env.MITOS_STATE_DIR), () => {
        if (coreReadsRecent()) return;
        refresh();
      }),
    );
    // Mining reads each harness's own history, so those writes refresh the charts.
    for (const path of resolveMiningWatchPaths())
      onCleanup(watchDatabase(path, bumpMining));
    for (const root of resolveHistoryWatchRoots())
      onCleanup(watchHistory(root, bumpMining));
    for (const signal of ["SIGTERM", "SIGHUP"] as const)
      process.on(signal, () => void quit());
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
      mutations: {
        reassignHarness,
        archiveThread,
        compactThread,
        deleteThread,
        noteThread,
        setMode,
        answerRequest,
        abortThread,
        forkThread,
        shareThread,
        diffThread,
        setAgent,
        setModel,
        listAgents,
        listModels,
        runCommand,
        hooksStatus,
        hooksInit,
      },
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
    if (
      harness &&
      detectInstalledHarnesses().includes(harness as KnownHarness)
    ) {
      void createAndSend(harness, parsed.text);
      return;
    }
    feedback.push({
      threadId: null,
      tone: FeedbackTone.Error,
      lines:
        'No harness selected. Run "/new <harness>" (e.g. "/new codex") to start a thread, or /help for commands.',
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
        loadHistory: () =>
          listUserMessages(process.env.MITOS_WORKSPACE_KEY, HISTORY_LIMIT),
        onDraftChange: suggestions.setDraft,
        registerSuggestionApplier: suggestions.registerApplier,
        suggestions: suggestions.suggestions,
        suggestionCursor: suggestions.cursor,
        working,
        pendingRequests: requests,
        usage,
        mining,
        focusRing,
      }}
    >
      <box
        flexDirection="column"
        width="100%"
        height="100%"
        backgroundColor={RGBA.defaultBackground()}
      >
        <Show when={error()}>
          {(message: Accessor<string>) => (
            <text fg={theme.err}>{message()}</text>
          )}
        </Show>
        <LayoutRenderer config={layout()} />
      </box>
    </AppStateProvider>
  );
}
