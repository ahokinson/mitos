mod queries;
mod windows;

use std::path::{Path, PathBuf};

use crate::config::{Plan, configured_plans, resolve_root};
use crate::domain::HarnessKind;
use crate::history::ReadonlyDatabase;
use crate::wire::requests::MiningRequest;
use crate::wire::responses::MiningReport;

use queries::{activity, agent_series, series, sessions, tool_series, totals};
use windows::est_windows;

const DATABASE_NAMES: [&str; 2] = ["opencode-stable.db", "opencode.db"];

pub(super) const SESSION_FILTER: &str =
    "(? IS NULL OR directory = ?) AND (? IS NULL OR time_created >= ?)";
pub(super) const JOINED_FILTER: &str =
    "(? IS NULL OR s.directory = ?) AND (? IS NULL OR s.time_created >= ?)";

/// `data_home` is the XDG data directory; the history lives under `opencode/`.
pub fn mine_opencode(data_home: &Path, request: &MiningRequest) -> MiningReport {
    let config_root = resolve_root(std::env::var_os("MITOS_STATE_DIR").map(PathBuf::from));
    let plan = configured_plans(&config_root)
        .into_iter()
        .find(|plan| plan.go_tier_label().is_some());
    let now = chrono::Utc::now().timestamp_millis();
    for name in DATABASE_NAMES {
        let path = data_home.join("opencode").join(name);
        let Some(database) = ReadonlyDatabase::open(&path) else {
            continue;
        };
        // A database without the expected tables is not this harness's.
        if let Ok(Some(report)) = read_report(&database, request, plan, now) {
            return report.attributed(HarnessKind::OpenCode.as_str());
        }
    }
    MiningReport::default()
}

fn read_report(
    database: &ReadonlyDatabase,
    request: &MiningRequest,
    plan: Option<Plan>,
    now: i64,
) -> anyhow::Result<Option<MiningReport>> {
    if database
        .query("SELECT id FROM session LIMIT 1", ())
        .is_err()
    {
        return Ok(None);
    }
    let (cost_by_agent, sessions_by_agent) = agent_series(database, request);
    let (tool_usage, tool_time) = tool_series(database, request);
    Ok(Some(MiningReport {
        totals: totals(database, request)?,
        harnesses: Vec::new(),
        cost_by_day: series(
            database,
            &format!(
                "SELECT date(time_created / 1000, 'unixepoch') AS label, \
                 ROUND(SUM(cost), 4) AS value \
                 FROM session WHERE {SESSION_FILTER} GROUP BY label ORDER BY label"
            ),
            request,
        ),
        cost_by_project: series(
            database,
            &format!(
                "SELECT directory AS label, ROUND(SUM(cost), 4) AS value \
                 FROM session WHERE {SESSION_FILTER} \
                 GROUP BY directory ORDER BY value DESC"
            ),
            request,
        ),
        cost_by_model: series(
            database,
            &format!(
                "SELECT json_extract(model, '$.id') AS label, ROUND(SUM(cost), 4) AS value \
                 FROM session WHERE {SESSION_FILTER} AND model IS NOT NULL \
                 GROUP BY label ORDER BY value DESC"
            ),
            request,
        ),
        cost_by_agent,
        cost_by_harness: Vec::new(),
        sessions_by_day: series(
            database,
            &format!(
                "SELECT date(time_created / 1000, 'unixepoch') AS label, COUNT(*) AS value \
                 FROM session WHERE {SESSION_FILTER} GROUP BY label ORDER BY label"
            ),
            request,
        ),
        sessions_by_agent,
        sessions_by_model: series(
            database,
            &format!(
                "SELECT json_extract(model, '$.id') AS label, COUNT(*) AS value \
                 FROM session WHERE {SESSION_FILTER} AND model IS NOT NULL \
                 GROUP BY label ORDER BY value DESC"
            ),
            request,
        ),
        tokens_by_day: series(
            database,
            &format!(
                "SELECT date(time_created / 1000, 'unixepoch') AS label, \
                 SUM(tokens_input + tokens_output + tokens_reasoning \
                     + tokens_cache_read + tokens_cache_write) AS value \
                 FROM session WHERE {SESSION_FILTER} GROUP BY label ORDER BY label"
            ),
            request,
        ),
        tool_usage,
        tool_time,
        activity_by_day: activity(database, request),
        sessions: sessions(database, request),
        est_windows: plan.and_then(|plan| est_windows(database, plan, now)),
    }))
}

#[cfg(test)]
mod tests {
    use std::fmt::Write as _;
    use std::path::PathBuf;

    use serde_json::json;

    use super::*;
    use crate::history::create_database;
    use crate::wire::requests::DEFAULT_TOP;
    use crate::wire::responses::{EstWindows, MinedSession, MiningSeriesPoint};

    const SCHEMA: &str = "CREATE TABLE session (
        id TEXT PRIMARY KEY, directory TEXT NOT NULL, title TEXT NOT NULL,
        cost REAL DEFAULT 0 NOT NULL, tokens_input INTEGER DEFAULT 0 NOT NULL,
        tokens_output INTEGER DEFAULT 0 NOT NULL, tokens_reasoning INTEGER DEFAULT 0 NOT NULL,
        tokens_cache_read INTEGER DEFAULT 0 NOT NULL, tokens_cache_write INTEGER DEFAULT 0 NOT NULL,
        model TEXT, agent TEXT, parent_id TEXT,
        time_created INTEGER NOT NULL, time_updated INTEGER NOT NULL);
        CREATE TABLE message (
        id TEXT PRIMARY KEY, session_id TEXT NOT NULL,
        time_created INTEGER NOT NULL, time_updated INTEGER NOT NULL, data TEXT NOT NULL);
        CREATE TABLE part (
        id TEXT PRIMARY KEY, message_id TEXT NOT NULL, session_id TEXT NOT NULL,
        time_created INTEGER NOT NULL, time_updated INTEGER NOT NULL, data TEXT NOT NULL);";

    struct Data {
        home: PathBuf,
        sql: String,
    }

    fn quote(text: &str) -> String {
        format!("'{}'", text.replace('\'', "''"))
    }

    fn near(left: f64, right: f64) -> bool {
        (left - right).abs() < f64::EPSILON
    }

    fn ms(iso: &str) -> String {
        chrono::DateTime::parse_from_rfc3339(iso)
            .unwrap()
            .with_timezone(&chrono::Utc)
            .timestamp_millis()
            .to_string()
    }

    fn now_ms() -> i64 {
        chrono::Utc::now().timestamp_millis()
    }

    impl Data {
        fn new() -> Self {
            let home = std::env::temp_dir().join(format!("mitos-mining-{}", crate::domain::id()));
            std::fs::create_dir_all(home.join("opencode")).unwrap();
            Self {
                home,
                sql: SCHEMA.into(),
            }
        }

        /// `extra` are `column = SQL value` pairs overriding the defaults.
        fn session(&mut self, id: &str, directory: &str, extra: &[(&str, String)]) -> &mut Self {
            let mut columns: Vec<(&str, String)> = vec![
                ("id", quote(id)),
                ("directory", quote(directory)),
                ("title", quote(&format!("title of {id}"))),
                ("time_created", "1000".into()),
                ("time_updated", "1000".into()),
            ];
            for (name, value) in extra {
                match columns.iter_mut().find(|(existing, _)| existing == name) {
                    Some(entry) => entry.1 = value.clone(),
                    None => columns.push((name, value.clone())),
                }
            }
            let names: Vec<_> = columns.iter().map(|(name, _)| *name).collect();
            let values: Vec<_> = columns.iter().map(|(_, value)| value.as_str()).collect();
            write!(
                self.sql,
                "INSERT INTO session ({}) VALUES ({});",
                names.join(", "),
                values.join(", ")
            )
            .unwrap();
            self
        }

        fn message(&mut self, session: &str, id: &str, at: &str, role: &str) -> &mut Self {
            let at = ms(at);
            write!(
                self.sql,
                "INSERT INTO message (id, session_id, time_created, time_updated, data) \
                 VALUES ({}, {}, {at}, {at}, {});",
                quote(id),
                quote(session),
                quote(&json!({ "role": role }).to_string())
            )
            .unwrap();
            self
        }

        fn part(&mut self, session: &str, id: &str, data: &str) -> &mut Self {
            write!(
                self.sql,
                "INSERT INTO part (id, message_id, session_id, time_created, time_updated, data) \
                 VALUES ({}, {}, {}, 1000, 1000, {});",
                quote(id),
                quote(id),
                quote(session),
                quote(data)
            )
            .unwrap();
            self
        }

        /// An assistant message whose model the plan-window estimate reads.
        fn assistant(&mut self, session: &str, id: &str, at: &str, model: &str) -> &mut Self {
            let at = ms(at);
            write!(
                self.sql,
                "INSERT INTO message (id, session_id, time_created, time_updated, data) \
                 VALUES ({}, {}, {at}, {at}, {});",
                quote(id),
                quote(session),
                quote(&json!({ "role": "assistant", "model": { "id": model, "providerID": "opencode-go" } }).to_string())
            )
            .unwrap();
            self
        }

        /// A `step-finish` part carrying a per-request cost, tied to a real
        /// message so the window estimate can join the model across.
        fn step(
            &mut self,
            session: &str,
            id: &str,
            message: &str,
            at: &str,
            cost: f64,
        ) -> &mut Self {
            let at = ms(at);
            write!(
                self.sql,
                "INSERT INTO part (id, message_id, session_id, time_created, time_updated, data) \
                 VALUES ({}, {}, {}, {at}, {at}, {});",
                quote(id),
                quote(message),
                quote(session),
                quote(&json!({ "type": "step-finish", "cost": cost }).to_string())
            )
            .unwrap();
            self
        }

        fn mine(&self, request: &MiningRequest) -> MiningReport {
            create_database(
                &self.home.join("opencode").join("opencode-stable.db"),
                &self.sql,
            );
            mine_opencode(&self.home, request)
        }
    }

    impl Drop for Data {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.home);
        }
    }

    fn default_request() -> MiningRequest {
        MiningRequest::new(None, None, DEFAULT_TOP)
    }

    fn labels(points: &[MiningSeriesPoint]) -> Vec<String> {
        points.iter().map(|p| p.label.clone()).collect()
    }

    fn ids(sessions: &[MinedSession]) -> Vec<String> {
        sessions.iter().map(|s| s.id.clone()).collect()
    }

    /// The one populated database every split assertion below mines.
    fn populated() -> Data {
        let mut data = Data::new();
        data.session(
            "ses_1",
            "/workspace",
            &[
                ("cost", "2.5".into()),
                ("tokens_input", "250".into()),
                ("tokens_output", "90".into()),
                ("tokens_reasoning", "40".into()),
                ("tokens_cache_read", "1000".into()),
                ("tokens_cache_write", "200".into()),
                (
                    "model",
                    quote(&json!({ "id": "glm-5.3", "providerID": "opencode-go" }).to_string()),
                ),
                ("agent", quote("build")),
                ("time_created", ms("2026-01-02T03:04:05Z")),
                ("time_updated", ms("2026-01-02T03:04:05Z")),
            ],
        )
        .session("ses_2", "/workspace", &[("cost", "1.25".into())])
        .session("ses_fork", "/workspace", &[("parent_id", quote("ses_1"))])
        .message("ses_1", "msg_1", "2026-01-02T03:04:05Z", "user")
        .message("ses_1", "msg_2", "2026-01-02T04:04:05Z", "assistant")
        .part(
            "ses_1",
            "p_1",
            &json!({ "type": "tool", "tool": "bash",
                "state": { "time": { "start": 0, "end": 10000 } } })
            .to_string(),
        )
        .part(
            "ses_1",
            "p_2",
            &json!({ "type": "tool", "tool": "bash",
                "state": { "time": { "start": 0, "end": 5000 } } })
            .to_string(),
        )
        .part(
            "ses_1",
            "p_3",
            &json!({ "type": "tool", "tool": "read",
                "state": { "time": { "start": 0, "end": 3000 } } })
            .to_string(),
        )
        .part(
            "ses_1",
            "p_4",
            &json!({ "type": "tool", "tool": "bash" }).to_string(),
        )
        .part(
            "ses_1",
            "p_5",
            &json!({
                "type": "patch",
                "files": ["/one.rs", "/two.rs"]
            })
            .to_string(),
        );
        data
    }

    /// A history of priced requests for the plan-window estimate: `now` is
    /// fixed so window boundaries are deterministic.
    fn priced() -> Data {
        let mut data = Data::new();
        data.assistant("ses_w", "m_glm", "2026-10-11T10:00:00Z", "glm-5.3")
            .step("ses_w", "p_glm", "m_glm", "2026-10-11T10:00:00Z", 12.0)
            .assistant("ses_w", "m_flash", "2026-10-11T09:00:00Z", "glm-5.3-flash")
            .step("ses_w", "p_flash", "m_flash", "2026-10-11T09:00:00Z", 12.0)
            .assistant("ses_w", "m_kimi", "2026-10-10T06:00:00Z", "kimi-k3")
            .step("ses_w", "p_kimi", "m_kimi", "2026-10-10T06:00:00Z", 30.0)
            .assistant(
                "ses_w",
                "m_free",
                "2026-10-11T11:50:00Z",
                "step-5-preview-free",
            )
            .step("ses_w", "p_free", "m_free", "2026-10-11T11:50:00Z", 99.0)
            .assistant("ses_w", "m_new", "2026-10-11T11:55:00Z", "zzz-unknown")
            .step("ses_w", "p_new", "m_new", "2026-10-11T11:55:00Z", 6.0)
            .assistant("ses_w", "m_old", "2026-09-30T12:00:00Z", "glm-5.3")
            .step("ses_w", "p_old", "m_old", "2026-09-30T12:00:00Z", 15.0);
        data
    }

    fn windows_at(plan: Plan, now: &str) -> Option<EstWindows> {
        let data = priced();
        let path = data.home.join("opencode").join("opencode-stable.db");
        create_database(&path, &data.sql);
        let database = ReadonlyDatabase::open(&path).unwrap();
        let now = ms(now).parse::<i64>().unwrap();
        read_report(&database, &default_request(), Some(plan), now)
            .unwrap()
            .unwrap()
            .est_windows
    }

    #[test]
    fn go_windows_burn_by_model_allowance_and_window_on_plus() {
        let windows = windows_at(Plan::OPENCODE_GO_PLUS, "2026-10-11T12:00:00Z").unwrap();
        assert_eq!(windows.tier, "plus");
        // glm-5.3 ($120) 12/120; flash ($180) 12/180; unknown ($240) 6/240;
        // the free preview and the pre-month row burn nothing.
        let expected_5h = 12.0 / 120.0 + 12.0 / 180.0 + 6.0 / 240.0;
        assert!(near(windows.burn_5h, expected_5h));
        assert_eq!(
            windows.oldest_5h_ms,
            Some(ms("2026-10-11T09:00:00Z").parse().unwrap())
        );
        // The kimi row is thirty hours old: outside 5h, inside the week.
        let expected_week = expected_5h + 30.0 / 60.0;
        assert!(near(windows.burn_week, expected_week));
        assert!(near(windows.burn_month, expected_week));
    }

    #[test]
    fn go_windows_scale_with_the_tier_allowances() {
        // The same $12 of glm-5.3 burns 12/15 on Go against 12/120 on Plus.
        let go = windows_at(Plan::OPENCODE_GO, "2026-10-11T12:00:00Z").unwrap();
        assert_eq!(go.tier, "go");
        let expected_5h = 12.0 / 15.0 + 12.0 / 60.0 + 6.0 / 60.0;
        assert!(near(go.burn_5h, expected_5h));
        let expected_week = expected_5h + 30.0 / 15.0;
        assert!(near(go.burn_week, expected_week));
    }

    #[test]
    fn go_windows_stay_absent_without_priced_requests() {
        let data = populated();
        let path = data.home.join("opencode").join("opencode-stable.db");
        create_database(&path, &data.sql);
        let database = ReadonlyDatabase::open(&path).unwrap();
        let report = read_report(
            &database,
            &default_request(),
            None,
            ms("2026-10-11T12:00:00Z").parse().unwrap(),
        )
        .unwrap()
        .unwrap();
        assert!(report.est_windows.is_none());
    }

    #[test]
    fn mines_totals_and_harness_attribution() {
        let report = populated().mine(&default_request());

        assert_eq!(report.totals.sessions, 3);
        assert_eq!(report.totals.messages, 2);
        assert_eq!(report.totals.messages_user, 1);
        assert_eq!(report.totals.messages_assistant, 1);
        assert_eq!(report.totals.tool_calls, 4);
        assert_eq!(report.totals.patches, 1);
        assert_eq!(report.totals.files_patched, 2);
        assert_eq!(report.totals.forks, 1);
        assert!(near(report.totals.cost_usd, 3.75));
        assert!(near(report.totals.tool_seconds, 18.0));
        assert_eq!(report.totals.tokens_input, 250);
        assert_eq!(report.totals.tokens_output, 90);
        assert_eq!(report.totals.tokens_reasoning, 40);
        assert_eq!(report.totals.tokens_cache_read, 1000);
        assert_eq!(report.totals.tokens_cache_write, 200);

        assert_eq!(
            report.harnesses,
            vec![crate::wire::responses::MiningHarness {
                label: "opencode".into(),
                sessions: 3,
                messages: 2,
                tool_calls: 4,
                patches: 1,
                cost_usd: 3.75,
                tokens_input: 250,
                tokens_output: 90,
                tokens_cache_read: 1000,
            }]
        );
        assert_eq!(
            report.cost_by_harness,
            vec![MiningSeriesPoint {
                label: "opencode".into(),
                value: json!(3.75)
            }]
        );
    }

    #[test]
    fn mines_grouped_and_day_series() {
        let report = populated().mine(&default_request());

        assert_eq!(
            labels(&report.cost_by_day),
            ["1970-01-01".to_string(), "2026-01-02".to_string()]
        );
        assert_eq!(
            report.cost_by_project,
            [MiningSeriesPoint {
                label: "/workspace".into(),
                value: json!(3.75)
            }]
        );
        assert_eq!(
            report.cost_by_model,
            [MiningSeriesPoint {
                label: "glm-5.3".into(),
                value: json!(2.5)
            }]
        );
        assert_eq!(
            report.cost_by_agent,
            [MiningSeriesPoint {
                label: "build".into(),
                value: json!(2.5)
            }]
        );
        assert_eq!(
            report.sessions_by_agent,
            [MiningSeriesPoint {
                label: "build".into(),
                value: json!(1)
            }]
        );
        assert_eq!(
            report.sessions_by_model,
            [MiningSeriesPoint {
                label: "glm-5.3".into(),
                value: json!(1)
            }]
        );
        assert_eq!(
            report.sessions_by_day,
            [
                MiningSeriesPoint {
                    label: "1970-01-01".into(),
                    value: json!(2)
                },
                MiningSeriesPoint {
                    label: "2026-01-02".into(),
                    value: json!(1)
                },
            ]
        );
        assert_eq!(
            labels(&report.tool_usage),
            ["bash".to_string(), "read".to_string()]
        );
        assert_eq!(report.tool_usage[0].value, json!(3));
        assert_eq!(
            report.tool_time,
            vec![
                MiningSeriesPoint {
                    label: "bash".into(),
                    value: json!(15.0)
                },
                MiningSeriesPoint {
                    label: "read".into(),
                    value: json!(3.0)
                },
            ]
        );
    }

    #[test]
    fn mines_activity_and_sessions() {
        let report = populated().mine(&default_request());

        let day = &report.activity_by_day[0];
        assert_eq!(day.label, "2026-01-02");
        assert_eq!(day.values.len(), 24);
        assert_eq!(day.values[3], 1);
        assert_eq!(day.values[4], 1);
        assert_eq!(day.values.iter().sum::<u64>(), 2);

        let session = &report.sessions[0];
        assert_eq!(session.id, "ses_1");
        assert_eq!(session.harness, "opencode");
        assert_eq!(session.title, "title of ses_1");
        assert_eq!(session.model.as_deref(), Some("glm-5.3"));
        assert_eq!(session.agent.as_deref(), Some("build"));
        assert_eq!(session.tokens_reasoning, 40);
        assert!(near(session.cost_usd, 2.5));
    }

    #[test]
    fn a_directory_filter_scopes_every_figure() {
        let mut data = Data::new();
        data.session("ses_in", "/workspace", &[("cost", "2.5".into())])
            .session("ses_out", "/elsewhere", &[("cost", "9.0".into())])
            .message("ses_in", "msg_1", "2026-01-02T03:04:05Z", "user")
            .message("ses_out", "msg_2", "2026-01-02T03:04:05Z", "assistant")
            .part(
                "ses_in",
                "p_1",
                &json!({ "type": "tool", "tool": "bash" }).to_string(),
            )
            .part(
                "ses_out",
                "p_2",
                &json!({ "type": "tool", "tool": "grep" }).to_string(),
            );

        let request = MiningRequest::new(Some("/workspace".into()), None, DEFAULT_TOP);
        let report = data.mine(&request);

        assert_eq!(report.totals.sessions, 1);
        assert_eq!(report.totals.messages, 1);
        assert_eq!(report.totals.messages_user, 1);
        assert_eq!(report.totals.messages_assistant, 0);
        assert_eq!(report.totals.tool_calls, 1);
        assert!(near(report.totals.cost_usd, 2.5));
        assert_eq!(labels(&report.tool_usage), ["bash".to_string()]);
        assert_eq!(ids(&report.sessions), ["ses_in".to_string()]);
    }

    #[test]
    fn a_since_filter_drops_sessions_created_before_the_window() {
        let old = (now_ms() - 40 * 86_400_000).to_string();
        let recent = now_ms().to_string();
        let mut data = Data::new();
        data.session("ses_old", "/workspace", &[("time_created", old)])
            .session("ses_new", "/workspace", &[("time_created", recent)]);

        let request = MiningRequest::new(None, Some(30), DEFAULT_TOP);
        let report = data.mine(&request);

        assert_eq!(report.totals.sessions, 1);
        assert_eq!(ids(&report.sessions), ["ses_new".to_string()]);
    }

    #[test]
    fn grouped_series_keep_every_entry_for_the_merge_to_trim() {
        let mut data = Data::new();
        data.session("ses_1", "/one", &[("cost", "1.0".into())])
            .session("ses_2", "/two", &[("cost", "2.0".into())])
            .session("ses_3", "/three", &[("cost", "3.0".into())]);

        let request = MiningRequest::new(None, None, 2);
        let report = data.mine(&request);

        assert_eq!(
            labels(&report.cost_by_project),
            ["/three".to_string(), "/two".to_string(), "/one".to_string()]
        );
        assert_eq!(report.sessions.len(), 3);
        assert_eq!(report.totals.sessions, 3);
    }

    #[test]
    fn no_database_yields_a_blank_report() {
        let data = Data::new();
        let report = data.mine(&default_request());
        assert_eq!(report, MiningReport::default());
    }
}
