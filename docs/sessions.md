# Sessions and handoffs

`Ctrl+N` opens a new draft. `/threads` lists the workspace's sessions and `/resume` switches to one. If `[session].default_harness` is set, the first message starts that harness; otherwise Mitos asks which installed harness to use. `/harness` reassigns the selected session's harness.

A Mitos session remains one session when its harness changes. Switching harnesses starts a fresh native target and supplies the target with the collected session evidence. Mitos mines evidence while turns stream, after an interactive harness exits, and immediately before a switch. It does not run a background collector.

The handoff is built from the durable event record and mined transcript material, not a generated summary. It lists the latest summary, decisions, open questions and the files the harness edited (read from its tool calls, relative to the workspace, up to 100), followed by the conversation. The default cap is 256 KiB (see `[handoff]` in [configuration](configuration.md)); to fit, it drops tool output first and then tool calls and status lines, and only if the messages alone still don't fit does it cut the middle, preserving the beginning and end and marking the omission.

`/compact` restarts the selected session on the same harness in a fresh native context. `/compact mechanical` (the default) re-seeds it from the bounded handoff above, with no model call. `/compact intelligent` first has the harness summarize the conversation in a separate throwaway session (read-only where the harness can enforce it, and removed afterwards), so the exchange never appears in the thread or in the harness's own history. It then seeds the fresh context with that summary plus everything after it. If no summary comes back, the session is left untouched.
