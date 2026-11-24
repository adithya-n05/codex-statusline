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
