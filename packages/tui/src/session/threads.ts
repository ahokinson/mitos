import { listThreads } from "@database/databases.ts";
import { createSignal } from "solid-js";

export enum ThreadStatus {
  Active = "active",
  Paused = "paused",
  Archived = "archived",
}

export enum ThreadMode {
  Plan = "plan",
  Build = "build",
}

export type Thread = {
  id: string;
  workspace_id: string;
  status: ThreadStatus;
  mode: ThreadMode;
  active_harness: string | null;
  native_session: unknown | null;
  last_event_seq: number;
  created_at: string;
  updated_at: string;
  /** First user message gives `/threads` and `/resume` a human-readable identity. */
  opening_message: string | null;
};

export type ThreadListState = {
  threads: () => Thread[];
  loading: () => boolean;
  refresh: () => void;
};

export function createThreadListState(
  workspaceKey: string | undefined,
): ThreadListState {
  const [threads, setThreads] = createSignal<Thread[]>([]);
  const [loading, setLoading] = createSignal(true);

  function refresh(): void {
    const next = listThreads(workspaceKey);
    setThreads((current) => (sameThreads(current, next) ? current : next));
    setLoading(false);
  }

  return { threads, loading, refresh };
}

/** Polling must preserve the current array when nothing changed. Replacing it
 * every tick makes Solid re-evaluate the selected thread and can visibly
 * redraw a terminal scrollbox despite there being no new conversation event. */
function sameThreads(
  current: readonly Thread[],
  next: readonly Thread[],
): boolean {
  return (
    current.length === next.length &&
    current.every((thread, index) => {
      const candidate = next[index];
      return (
        candidate !== undefined &&
        thread.id === candidate.id &&
        thread.workspace_id === candidate.workspace_id &&
        thread.status === candidate.status &&
        thread.mode === candidate.mode &&
        thread.active_harness === candidate.active_harness &&
        thread.last_event_seq === candidate.last_event_seq &&
        thread.created_at === candidate.created_at &&
        thread.updated_at === candidate.updated_at &&
        thread.opening_message === candidate.opening_message &&
        JSON.stringify(thread.native_session) ===
          JSON.stringify(candidate.native_session)
      );
    })
  );
}
