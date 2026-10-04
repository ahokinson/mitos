# Sessions and handoffs

`Ctrl+N` opens a new draft. `/threads` lists the workspace's sessions and `/resume` switches to one. If `[session].default_harness` is set, the first message starts that harness; otherwise Mitos asks which installed harness to use. `/harness` reassigns the selected session's harness.

A Mitos session remains one session when its harness changes. Switching harnesses starts a fresh native target and supplies the target with the collected session evidence. Mitos mines evidence while turns stream, after an interactive harness exits, and immediately before a switch. It does not run a background collector.

The handoff is built from the durable event record and mined transcript material, not a generated summary. The default inline cap is 48 KiB; when it must omit material it preserves the beginning and end and marks the omission.

`/compact` restarts the selected session on the same harness in a fresh native context. `/compact mechanical` (the default) re-seeds it from the bounded handoff above, with no model call. `/compact intelligent` first asks the harness to summarize the conversation, then seeds the fresh context with that summary plus everything after it. If no summary comes back, the session is left untouched.
