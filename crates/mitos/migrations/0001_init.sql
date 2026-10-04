-- Greenfield: this is the only migration.

CREATE TABLE workspaces (
    id TEXT PRIMARY KEY,
    root TEXT NOT NULL UNIQUE,
    git_dir TEXT,
    workspace_key TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE threads (
    id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL REFERENCES workspaces(id),
    status TEXT NOT NULL CHECK (status IN ('active', 'paused', 'archived')) DEFAULT 'active',
    mode TEXT NOT NULL CHECK (mode IN ('plan', 'build')) DEFAULT 'build',
    active_harness TEXT,
    native_session TEXT,
    last_event_seq INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE INDEX idx_threads_workspace ON threads(workspace_id);

CREATE TABLE harness_bindings (
    id TEXT PRIMARY KEY,
    thread_id TEXT NOT NULL REFERENCES threads(id),
    harness TEXT NOT NULL,
    native_session TEXT,
    bound_at TEXT NOT NULL,
    unbound_at TEXT,
    unbind_reason TEXT CHECK (unbind_reason IN ('reassigned', 'archived'))
);
CREATE INDEX idx_harness_bindings_thread ON harness_bindings(thread_id);

CREATE TABLE thread_events (
    thread_id TEXT NOT NULL REFERENCES threads(id),
    seq INTEGER NOT NULL,
    turn_id TEXT,
    harness TEXT,
    kind TEXT NOT NULL CHECK (kind IN (
        'user_message', 'assistant_message', 'assistant_delta',
        'tool_call', 'tool_result', 'status', 'usage', 'error',
        'note', 'decision', 'question',
        'request_opened', 'request_answered', 'mode_changed',
        'thread_created', 'harness_bound', 'harness_unbound', 'handoff_carryover',
        'compaction'
    )),
    role TEXT,
    content TEXT,
    payload_json TEXT,
    created_at TEXT NOT NULL,
    PRIMARY KEY (thread_id, seq)
);

-- Harness-initiated asks (questions, permission prompts, plan approvals).
CREATE TABLE harness_requests (
    id TEXT PRIMARY KEY,
    thread_id TEXT NOT NULL REFERENCES threads(id),
    turn_id TEXT,
    harness TEXT,
    kind TEXT NOT NULL CHECK (kind IN ('question', 'permission', 'plan_approval')),
    payload_json TEXT,
    status TEXT NOT NULL CHECK (status IN ('pending', 'answered', 'cancelled')) DEFAULT 'pending',
    response_json TEXT,
    created_at TEXT NOT NULL,
    answered_at TEXT
);
CREATE INDEX idx_harness_requests_thread ON harness_requests(thread_id, status);

CREATE TABLE usage_snapshots (
    id TEXT PRIMARY KEY,
    thread_id TEXT NOT NULL REFERENCES threads(id),
    harness TEXT NOT NULL,
    turn_id TEXT,
    observed_at TEXT NOT NULL,
    input_tokens INTEGER,
    output_tokens INTEGER,
    cached_input_tokens INTEGER,
    cost_usd REAL,
    context_used_tokens INTEGER,
    context_limit_tokens INTEGER,
    model TEXT,
    turns INTEGER
);
CREATE INDEX idx_usage_snapshots_thread_time ON usage_snapshots(thread_id, observed_at);

-- Latest session-level reading a harness hook reported, one row per native
-- session. `native_session` is '' when a hook carried no session id.
CREATE TABLE hook_observations (
    harness TEXT NOT NULL,
    native_session TEXT NOT NULL,
    thread_id TEXT,
    cwd TEXT,
    model TEXT,
    event TEXT,
    cost_usd REAL,
    context_used_tokens INTEGER,
    context_limit_tokens INTEGER,
    observed_at TEXT NOT NULL,
    PRIMARY KEY (harness, native_session)
);
CREATE INDEX idx_hook_observations_thread ON hook_observations(thread_id);

-- Account-wide, keyed by harness alone, not by thread.
CREATE TABLE harness_plan_usage (
    harness TEXT PRIMARY KEY,
    plan_five_hour_percent REAL,
    plan_five_hour_resets_at TEXT,
    plan_week_percent REAL,
    plan_week_resets_at TEXT,
    observed_at TEXT NOT NULL
);

CREATE TABLE handoff_carryovers (
    id TEXT PRIMARY KEY,
    thread_id TEXT NOT NULL REFERENCES threads(id),
    from_harness TEXT,
    to_harness TEXT NOT NULL,
    created_at TEXT NOT NULL,
    note TEXT,
    decisions_json TEXT NOT NULL DEFAULT '[]',
    questions_json TEXT NOT NULL DEFAULT '[]',
    bounded_context TEXT NOT NULL,
    source_event_seq_high_watermark INTEGER
);
CREATE INDEX idx_handoff_carryovers_thread ON handoff_carryovers(thread_id);
