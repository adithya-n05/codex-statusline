use codex_statusline::protocol;
use codex_statusline::statusline;

use std::{env, io, process::ExitCode};

fn main() -> io::Result<ExitCode> {
    let mut args = env::args_os().skip(1);
    match (args.next(), args.next()) {
        (None, None) => {
            let identity = codex_statusline::account::codex_home()
                .map(|home| codex_statusline::account::Identity::load(&home))
                .unwrap_or_default();
            let home = codex_statusline::account::codex_home();
            let mut history = codex_statusline::history::History::default();
            let account_id = match &identity {
                codex_statusline::account::Identity::ChatGpt(id) => id.clone(),
                _ => None,
            };
            let live_account = account_id.clone();
            let mut active_credits = None;
            let mut live_ledger = codex_statusline::credits::Ledger::default();
            let mut ledger = codex_statusline::credits::Ledger::default();
            let mut worker = codex_statusline::runtime::Worker::new(move |state| {
                let cwd = state
                    .pointer("/thread/workspace/cwd")
                    .and_then(serde_json::Value::as_str);
                let thread_id = state["thread"]["id"].as_str();
                let owner = home
                    .as_deref()
                    .zip(thread_id)
                    .zip(account_id.as_deref())
                    .filter(|((h, id), account)| {
                        codex_statusline::storage::account_matches(h, id, account)
                    })
                    .map(|((h, _), account)| (h, account));
                let metrics = home
                    .as_deref()
                    .zip(thread_id)
                    .and_then(|(h, id)| codex_statusline::storage::rollout(h, id))
                    .and_then(|p| {
                        history.read_with(&p, |event| {
                            if owner.is_some() {
                                ledger.observe(event)
                            }
                        })
                    })
                    .cloned()
                    .unwrap_or_default();
                if let Some((h, id)) = owner {
                    let _ = ledger.sync(h, id);
                }
                codex_statusline::runtime::Extra {
                    history: metrics,
                    saved_name: home
                        .as_deref()
                        .zip(thread_id)
                        .and_then(|(h, id)| codex_statusline::storage::saved_name(h, id)),
                    credits: None,
                    compaction: home.as_deref().and_then(|h| {
                        codex_statusline::compaction::Sample::load(h, &state["thread"])
                    }),
                    git: cwd.and_then(|p| codex_statusline::git::summary(std::path::Path::new(p))),
                }
            });
            let local_home = codex_statusline::account::codex_home();
            let mut startup_sample = None;
            let mut recent_history = codex_statusline::history::RecentHistory::default();
            protocol::serve(io::stdin().lock(), io::stdout().lock(), |s| {
                let mut extra = worker.latest(s);
                let t = &s["thread"];
                let native_context_missing = t
                    .pointer("/usage/context/used_percentage")
                    .is_none_or(|v| v.is_null());
                let awaiting_worker = extra.history.tools_label().is_none();
                if awaiting_worker || (extra.compaction.is_none() && native_context_missing) {
                    let key = (
                        t["id"].clone(),
                        t["model"]["id"].clone(),
                        t["usage"]["context"]["context_window_size"].clone(),
                    );
                    if startup_sample.as_ref().is_none_or(|(k, _)| k != &key) {
                        let sample = local_home
                            .as_deref()
                            .and_then(|h| codex_statusline::compaction::Sample::load(h, t));
                        startup_sample = Some((key, sample));
                    }
                    if extra.compaction.is_none() {
                        extra.compaction = startup_sample
                            .as_ref()
                            .and_then(|(_, sample)| sample.clone());
                    }
                }
                let owns_rollout = local_home
                    .as_deref()
                    .zip(t["id"].as_str())
                    .zip(live_account.as_deref())
                    .is_some_and(|((h, id), account)| {
                        codex_statusline::storage::account_matches(h, id, account)
                    });
                if let Some(home) = local_home.as_deref()
                    && let Some(id) = t["id"].as_str()
                    && let Some(path) = codex_statusline::storage::rollout(home, id)
                    && let Some(recent) = recent_history.read_with(&path, |event| {
                        if owns_rollout {
                            live_ledger.observe(event);
                        }
                    })
                {
                    if awaiting_worker
                        || (native_context_missing
                            && recent
                                .context
                                .restore(&t["model"]["id"], &t["usage"]["context"])
                                .is_some())
                    {
                        extra.history.context = recent.context.clone();
                    }
                    extra.history.merge_recent_speed(recent, home, id);
                }
                if let Some((home, account)) = local_home.as_deref().zip(live_account.as_deref()) {
                    let _ = live_ledger.sync_with_timeout(
                        home,
                        account,
                        std::time::Duration::from_millis(15),
                    );
                    let reset = codex_statusline::usage::weekly_window(&s["account"])
                        .and_then(|w| w["resets_at"].as_i64());
                    let fresh = match reset {
                        Some(at) => codex_statusline::credits::Standing::read(home, account, at),
                        None => codex_statusline::credits::Standing::active(
                            home,
                            account,
                            codex_statusline::usage::unix_now(),
                        ),
                    };
                    if fresh.is_some() {
                        active_credits = fresh;
                    }
                    extra.credits.clone_from(&active_credits);
                }
                statusline::render_enriched(s, &identity, &extra)
            })?;
            Ok(ExitCode::SUCCESS)
        }
        (Some(flag), None) if flag == "--version" => {
            println!("codex-statusline {}", env!("CARGO_PKG_VERSION"));
            Ok(ExitCode::SUCCESS)
        }
        _ => {
            eprintln!("Usage: codex-statusline [--version]");
            Ok(ExitCode::from(2))
        }
    }
}
