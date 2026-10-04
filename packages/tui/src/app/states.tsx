import { type ParentProps, createContext, useContext } from "solid-js"

import type { CommandSuggestion } from "@commands/suggestions.ts"
import type { ThreadEventsState } from "@session/events.ts"
import type { FeedbackState } from "@commands/feedbackLines.ts"
import type { HarnessRequest } from "@session/requests.ts"
import type { Thread, ThreadListState } from "@session/threads.ts"
import type { UsageSnapshot } from "@database/views.ts"
import type { FocusRing } from "@input/focusRings.ts"

/** Shared state behind the layout tree's widgets — they pull what they
 * need from here via `useAppState()` rather than the generic layout
 * renderer threading bespoke props per widget. Widgets can use only the data
 * they need without coupling the layout system to a specific arrangement. */
export type AppState = {
  list: ThreadListState
  selected: () => Thread | null
  currentHarness: () => string | null
  events: ThreadEventsState
  feedback: FeedbackState
  onSend: (text: string) => void
  loadHistory: () => readonly string[]
  onDraftChange: (text: string) => void
  registerSuggestionApplier: (apply: (text: string) => void) => void
  suggestions: () => readonly CommandSuggestion[]
  suggestionCursor: () => number
  working: () => boolean
  pendingRequests: () => readonly HarnessRequest[]
  usage: () => UsageSnapshot | null
  focusRing: FocusRing
}

const AppStateContext = createContext<AppState>()

export function AppStateProvider(props: ParentProps & { value: AppState }) {
  return <AppStateContext.Provider value={props.value}>{props.children}</AppStateContext.Provider>
}

export function useAppState(): AppState {
  const value = useContext(AppStateContext)
  if (!value) throw new Error("useAppState must be used within an AppStateProvider")
  return value
}
