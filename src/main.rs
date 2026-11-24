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
            let mut active_credits =
                home.as_deref()
                    .zip(account_id.as_deref())
                    .and_then(|(h, id)| {
                        codex_statusline::credits::Standing::active(
                            h,
                            id,
                            codex_statusline::usage::unix_now(),
                        )
                    });
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
                    credits: owner.and_then(|(h, id)| {
                        let reset =
                            codex_statusline::usage::weekly_window(&state["account"])?["resets_at"]
                                .as_i64()?;
                        codex_statusline::credits::Standing::read(h, id, reset)
                    }),
                    compaction: home.as_deref().and_then(|h| {
                        codex_statusline::compaction::Sample::load(h, &state["thread"])
                    }),
                    git: cwd.and_then(|p| codex_statusline::git::summary(std::path::Path::new(p))),
                }
            });
            let local_home = codex_statusline::account::codex_home();
            let mut startup_sample = None;
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
                    if startup_sample.as_ref().is_none_or(|(k, _, _)| k != &key) {
                        let sample = local_home
                            .as_deref()
                            .and_then(|h| codex_statusline::compaction::Sample::load(h, t));
                        let recent = local_home
                            .as_deref()
                            .zip(t["id"].as_str())
                            .and_then(|(h, id)| codex_statusline::storage::rollout(h, id))
                            .and_then(|p| codex_statusline::history::recent(&p));
                        startup_sample = Some((key, sample, recent));
                    }
                    if let Some((_, sample, recent)) = &startup_sample {
                        if extra.compaction.is_none() {
                            extra.compaction = sample.clone();
                        }
                        if awaiting_worker && let Some(recent) = recent {
                            extra.history.context = recent.context.clone();
                            extra.history.speed = recent.speed.clone();
                        }
                    }
                }
                if let Some(fresh) = &extra.credits {
                    active_credits = Some(fresh.clone());
                } else {
                    extra.credits = active_credits.clone();
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
