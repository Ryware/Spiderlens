//! One authorised call to a Google API, shared by URL Inspection and
//! Analytics (Search Console keeps its own copy in `gsc`).

use reqwest::Method;
use rusqlite::Connection;
use serde_json::Value;

use crate::Result;
use crate::http::HttpError;
use crate::limits::with_retry;
use crate::oauth::Session;
use crate::run::Ctx;

/// Calls the API with the session's token: waits for the rate bucket,
/// refreshes the token once on 401 and applies spec 004's retry rules to
/// transient failures. Returns the answer and the attempts used.
pub(crate) async fn call(
    ctx: &Ctx,
    session: &Session,
    name: &str,
    method: Method,
    url: &str,
    body: Option<&Value>,
) -> (std::result::Result<Value, HttpError>, u32) {
    let mut refreshed = false;
    loop {
        ctx.bucket.acquire().await;
        let (r, attempts) = with_retry(|| async {
            let token = session.bearer().await?;
            let mut req = ctx.http.request(method.clone(), url).bearer_auth(token);
            if let Some(b) = body {
                req = req
                    .header(reqwest::header::CONTENT_TYPE, "application/json")
                    .body(b.to_string());
            }
            ctx.http.json(name, req).await
        })
        .await;
        match r {
            Err(HttpError::Unauthorized { .. }) if !refreshed => {
                session.invalidate();
                refreshed = true;
            }
            other => return (other, attempts),
        }
    }
}

/// Runs `f` in one transaction on the crawl file: committed when it
/// succeeds, rolled back when it fails, so a run never leaves half rows.
pub(crate) fn in_tx<T>(conn: &Connection, f: impl FnOnce() -> Result<T>) -> Result<T> {
    conn.execute_batch("BEGIN IMMEDIATE")?;
    match f() {
        Ok(v) => {
            conn.execute_batch("COMMIT")?;
            Ok(v)
        }
        Err(e) => {
            let _ = conn.execute_batch("ROLLBACK");
            Err(e)
        }
    }
}
