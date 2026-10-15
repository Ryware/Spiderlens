use std::sync::Mutex;
use std::sync::atomic::AtomicBool;

use super::*;

/// Counts pages into `<schema>.fake_counts`; `b` reads `a`'s output.
struct Fake {
    id: &'static str,
    version: u32,
    after: &'static [&'static str],
    needs: &'static [Need],
    runs: Mutex<u32>,
}

impl Fake {
    const fn new(id: &'static str, after: &'static [&'static str]) -> Self {
        Self {
            id,
            version: 1,
            after,
            needs: &[],
            runs: Mutex::new(0),
        }
    }

    fn runs(&self) -> u32 {
        *self.runs.lock().unwrap()
    }
}

impl Pass for Fake {
    fn id(&self) -> &'static str {
        self.id
    }
    fn version(&self) -> u32 {
        self.version
    }
    fn after(&self) -> &'static [&'static str] {
        self.after
    }
    fn needs(&self) -> &'static [Need] {
        self.needs
    }
    fn budget(&self) -> Duration {
        Duration::from_secs(1)
    }
    fn run(&self, cx: &mut PassContext<'_>) -> Result<()> {
        *self.runs.lock().unwrap() += 1;
        let s = cx.schema;
        let src = if self.after.is_empty() {
            "SELECT COUNT(*) FROM urls".to_string()
        } else {
            format!("SELECT n * 10 FROM {s}.out_{}", self.after[0])
        };
        cx.conn.execute_batch(&format!(
            "DROP TABLE IF EXISTS {s}.out_{id}; CREATE TABLE {s}.out_{id} AS SELECT ({src}) AS n;",
            id = self.id
        ))?;
        cx.progress(1, 1, self.id)
    }
}

fn file() -> (tempfile::TempDir, Connection) {
    let dir = tempfile::tempdir().unwrap();
    let conn = Connection::open(dir.path().join("c.spiderlens")).unwrap();
    conn.execute_batch(
        "CREATE TABLE meta(key TEXT PRIMARY KEY, value TEXT);
         CREATE TABLE urls(id INTEGER PRIMARY KEY, fetched_at INTEGER);
         INSERT INTO urls VALUES (1, 100), (2, 100), (3, NULL);",
    )
    .unwrap();
    (dir, conn)
}

fn out(conn: &Connection, t: &str) -> i64 {
    conn.query_row(&format!("SELECT n FROM {t}"), [], |r| r.get(0))
        .unwrap()
}

#[test]
fn runs_in_dependency_order_then_only_when_stale() {
    let (_d, conn) = file();
    let a = Fake::new("a", &[]);
    let b = Fake::new("b", &["a"]);
    let c = Fake::new("c", &[]);
    // b is listed first but reads a.
    let passes: [&dyn Pass; 3] = [&b, &a, &c];
    let ids: Vec<_> = order(&passes).unwrap().iter().map(|p| p.id()).collect();
    assert_eq!(ids, ["a", "b", "c"]);

    let mut events = Vec::new();
    let no = AtomicBool::new(false);
    let r = run(&conn, "main", &passes, &mut |e| events.push(e.pass), &no).unwrap();
    assert!(r.saved);
    assert_eq!(
        r.ran.iter().map(|(id, _)| id.as_str()).collect::<Vec<_>>(),
        ["a", "b", "c"]
    );
    assert_eq!(events, ["a", "b", "c"]);
    assert_eq!(out(&conn, "out_b"), 30);
    assert!(records(&conn).unwrap().contains_key("b"));

    // Nothing changed: nothing reruns.
    let r = run(&conn, "main", &passes, &mut |_| {}, &no).unwrap();
    assert!(r.ran.is_empty());

    // More URLs crawled (a resume): everything is stale.
    conn.execute("UPDATE urls SET fetched_at = 200 WHERE id = 3", [])
        .unwrap();
    run(&conn, "main", &passes, &mut |_| {}, &no).unwrap();
    assert_eq!((a.runs(), b.runs(), c.runs()), (2, 2, 2));

    // A new version of a reruns it and b, which reads it, but not c.
    let a2 = Fake {
        version: 2,
        ..Fake::new("a", &[])
    };
    let passes: [&dyn Pass; 3] = [&b, &a2, &c];
    let r = run(&conn, "main", &passes, &mut |_| {}, &no).unwrap();
    assert_eq!(
        r.ran.iter().map(|(id, _)| id.as_str()).collect::<Vec<_>>(),
        ["a", "b"]
    );
    assert_eq!(c.runs(), 2);
}

#[test]
fn cancel_keeps_earlier_passes_and_rolls_back_the_current_one() {
    let (_d, conn) = file();
    let a = Fake::new("a", &[]);
    let b = Fake::new("b", &["a"]);
    let passes: [&dyn Pass; 2] = [&a, &b];
    let cancel = AtomicBool::new(false);
    let err = run(
        &conn,
        "main",
        &passes,
        &mut |e| {
            if e.pass == "a" {
                cancel.store(true, Ordering::Relaxed);
            }
        },
        &cancel,
    );
    assert!(matches!(err, Err(Error::Cancelled)));
    // a's progress call saw the cancel, so a rolled back too.
    let recs = records(&conn).unwrap();
    assert!(recs.is_empty());
    let exists = |t: &str| {
        conn.prepare("SELECT 1 FROM sqlite_master WHERE name = ?1")
            .unwrap()
            .exists([t])
            .unwrap()
    };
    assert!(!exists("out_a"));

    // Cancelled during b: a stays done and is not rerun.
    let cancel = AtomicBool::new(false);
    let err = run(
        &conn,
        "main",
        &passes,
        &mut |e| {
            if e.pass == "b" {
                cancel.store(true, Ordering::Relaxed);
            }
        },
        &cancel,
    );
    assert!(matches!(err, Err(Error::Cancelled)));
    assert!(exists("out_a") && !exists("out_b"));
    run(&conn, "main", &passes, &mut |_| {}, &AtomicBool::new(false)).unwrap();
    assert_eq!((a.runs(), b.runs()), (2, 2));
    assert!(exists("out_a") && exists("out_b"));
}

#[test]
fn read_only_runs_into_temp_and_saves_nothing() {
    let (_d, conn) = file();
    let a = Fake::new("a", &[]);
    let passes: [&dyn Pass; 1] = [&a];
    let r = run(&conn, "temp", &passes, &mut |_| {}, &AtomicBool::new(false)).unwrap();
    assert!(!r.saved);
    assert_eq!(out(&conn, "temp.out_a"), 3);
    assert!(records(&conn).unwrap().is_empty());
}

#[test]
fn missing_data_skips_with_the_reason() {
    let (_d, conn) = file();
    let a = Fake {
        needs: &[Need::Extraction],
        ..Fake::new("a", &[])
    };
    let passes: [&dyn Pass; 1] = [&a];
    let r = run(&conn, "main", &passes, &mut |_| {}, &AtomicBool::new(false)).unwrap();
    assert!(r.ran.is_empty());
    assert_eq!(r.skipped[0].0, "a");
    assert_eq!(a.runs(), 0);
}

#[test]
fn bad_dependencies_are_errors() {
    let a = Fake::new("a", &["b"]);
    let b = Fake::new("b", &["a"]);
    assert!(order(&[&a as &dyn Pass, &b]).is_err());
    assert!(order(&[&a as &dyn Pass]).is_err());
}
