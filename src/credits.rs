use rusqlite::{Connection, params};
use serde_json::Value;
use std::path::Path;

struct Observation {
    at: String,
    reset: i64,
    balance: f64,
}

#[derive(Default)]
pub struct Ledger {
    pending: Vec<Observation>,
}

impl Ledger {
    pub fn observe(&mut self, event: &Value) {
        if event["type"] != "event_msg" || event["payload"]["type"] != "token_count" {
            return;
        }
        let r = &event["payload"]["rate_limits"];
        let credits = &r["credits"];
        if r["limit_id"] != "codex" || credits["unlimited"] == true {
            return;
        }
        let Some(reset) = [&r["primary"], &r["secondary"]]
            .into_iter()
            .find(|w| w["window_minutes"].as_i64() == Some(10080))
            .and_then(|w| w["resets_at"].as_i64())
        else {
            return;
        };
        let Some(balance) = credits["balance"]
            .as_f64()
            .or_else(|| credits["balance"].as_str()?.parse().ok())
            .filter(|n| n.is_finite() && *n >= 0.)
        else {
            return;
        };
        if let Some(at) = event["timestamp"].as_str() {
            self.pending.push(Observation {
                at: at.to_owned(),
                reset,
                balance,
            });
        }
    }
    pub fn sync(&mut self, home: &Path, account: &str) -> rusqlite::Result<()> {
        if self.pending.is_empty() {
            return Ok(());
        }
        let dir = home.join("codex-statusline");
        let mut builder = std::fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        if !dir.is_dir()
            && let Err(e) = builder.create(&dir)
            && e.kind() != std::io::ErrorKind::AlreadyExists
        {
            return Err(rusqlite::Error::InvalidPath(dir));
        }
        let mut db = Connection::open(dir.join("credits.sqlite3"))?;
        db.busy_timeout(std::time::Duration::from_millis(250))?;
        let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        tx.execute_batch("CREATE TABLE IF NOT EXISTS observations(account TEXT NOT NULL,reset INTEGER NOT NULL,at INTEGER NOT NULL,balance REAL NOT NULL,PRIMARY KEY(account,reset,at)) WITHOUT ROWID;")?;
        {
            let mut put=tx.prepare("INSERT OR IGNORE INTO observations(account,reset,at,balance) SELECT ?1, ?2, CAST(ROUND((julianday(?3)-2440587.5)*86400000) AS INTEGER), ?4 WHERE CAST(strftime('%s',?3) AS INTEGER)>=?2-604800 AND CAST(strftime('%s',?3) AS INTEGER)<?2")?;
            for sample in &self.pending {
                put.execute(params![account, sample.reset, sample.at, sample.balance])?;
            }
        }
        tx.commit()?;
        self.pending.clear();
        Ok(())
    }
}
#[derive(Clone)]
pub struct Standing {
    pub reset: i64,
    balance: f64,
    base: f64,
}
// Separate Codex responses derive the same weekly boundary from relative clocks.
// Allow a minute of skew while keeping consecutive weeks distinct.
const RESET_TOLERANCE_SECONDS: i64 = 60;
impl Standing {
    pub fn active(home: &Path, account: &str, now: i64) -> Option<Self> {
        let db = Connection::open_with_flags(
            home.join("codex-statusline/credits.sqlite3"),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .ok()?;
        let reset = db
            .query_row(
                "SELECT reset FROM observations WHERE account=?1 AND reset>?2 AND reset<=?3 ORDER BY at DESC, reset LIMIT 1",
                params![
                    account,
                    now,
                    now.saturating_add(604800 + RESET_TOLERANCE_SECONDS)
                ],
                |r| r.get::<_, i64>(0),
            )
            .ok()?;
        Self::read(home, account, reset)
    }
    pub fn matches_reset(&self, live: i64) -> bool {
        self.reset.abs_diff(live) <= RESET_TOLERANCE_SECONDS as u64
    }
    pub fn read(home: &Path, account: &str, reset: i64) -> Option<Self> {
        let db = Connection::open_with_flags(
            home.join("codex-statusline/credits.sqlite3"),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .ok()?;
        // Reconstruct in observation order so replaying another chat cannot invent a top up.
        let mut query = db
            .prepare("SELECT balance FROM observations WHERE account=?1 AND reset BETWEEN ?2 AND ?3 ORDER BY at, reset")
            .ok()?;
        let mut rows = query
            .query_map(
                params![
                    account,
                    reset.saturating_sub(RESET_TOLERANCE_SECONDS),
                    reset.saturating_add(RESET_TOLERANCE_SECONDS)
                ],
                |r| r.get::<_, f64>(0),
            )
            .ok()?;
        let mut balance = rows.next()?.ok()?;
        let mut base = balance;
        let mut spent = false;
        for row in rows {
            let next = row.ok()?;
            if next < balance {
                spent = true;
            } else {
                base += next - balance;
            }
            balance = next;
        }
        (spent && base > 0.).then_some(Self {
            reset,
            balance,
            base,
        })
    }
    pub fn render(&self, now: i64) -> Vec<Value> {
        let remaining = (100. * self.balance / self.base).clamp(0., 100.);
        vec![
            serde_json::json!(format!(
                "CRDTS {} / {} · {remaining:.0}% left ",
                amount(self.balance),
                amount(self.base)
            )),
            crate::layout::gauge(remaining, 100. - remaining),
            serde_json::json!(format!(
                " · reset {}",
                crate::usage::countdown(self.reset.saturating_sub(now))
            )),
        ]
    }
}
fn amount(value: f64) -> String {
    let digits = format!("{:.0}", value.round());
    let mut label = String::new();
    for (i, ch) in digits.char_indices() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            label.push(',');
        }
        label.push(ch);
    }
    label
}
