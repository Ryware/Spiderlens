//! A fixture site with a login page and a member area (spec 025 US2).
//!
//! * `GET` or `POST /login?user=ann` signs in: it sets the `HttpOnly`
//!   cookie `sid=<random>` and redirects to `/members/`. Without `user` it
//!   shows the login form.
//! * `/members/` and `/members/<i>` need the cookie, else they redirect to
//!   `/login`. `/members/` links to every member page and to `/logout`.
//! * `/logout` ends the session on the server and redirects to `/login`.
//! * `/account` says "Hello <user>" when signed in and shows a "Sign in"
//!   link when not (a signed-out check for the crawl).
//! * [`LoginSiteBuilder::expire_after`] ends every session after the n-th
//!   request, as a site that times a session out.
//!
//! Like the other fixtures it answers for any `Host`; tell the crawler to
//! resolve [`LoginSite::host`] to it.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use crate::auth_site::{Reply, Request, random_token, serve};

/// One logged request.
#[derive(Clone, Debug)]
pub struct LoginHit {
    /// Path without the query.
    pub path: String,
    /// The `Cookie` header, if sent.
    pub cookie: Option<String>,
    /// Whether the cookie held a live session when the request arrived.
    pub signed_in: bool,
    /// The request's number, from 1, over the whole life of the site.
    pub number: usize,
}

struct State {
    members: usize,
    expire_after: Option<usize>,
    /// `sid` to user name.
    sessions: Mutex<HashMap<String, String>>,
    log: Mutex<Vec<LoginHit>>,
    requests: AtomicUsize,
    counter: AtomicUsize,
    expired_at: Mutex<Option<usize>>,
}

pub struct LoginSiteBuilder {
    host: String,
    members: usize,
    expire_after: Option<usize>,
}

impl LoginSiteBuilder {
    /// How many pages `/members/` links to (default 3).
    pub fn members(mut self, n: usize) -> Self {
        self.members = n;
        self
    }

    /// Ends every session once the site has answered `n` requests: the
    /// n-th request is still served signed in.
    pub fn expire_after(mut self, n: usize) -> Self {
        self.expire_after = Some(n);
        self
    }

    pub fn start(self) -> LoginSite {
        let state = Arc::new(State {
            members: self.members,
            expire_after: self.expire_after,
            sessions: Mutex::new(HashMap::new()),
            log: Mutex::new(Vec::new()),
            requests: AtomicUsize::new(0),
            counter: AtomicUsize::new(0),
            expired_at: Mutex::new(None),
        });
        let shared = state.clone();
        let addr = serve(move |req: &Request, _conn: usize, _s: &mut ()| handle(&shared, req));
        LoginSite {
            addr,
            host: self.host,
            state,
        }
    }
}

pub struct LoginSite {
    addr: SocketAddr,
    host: String,
    state: Arc<State>,
}

impl LoginSite {
    pub fn builder(host: &str) -> LoginSiteBuilder {
        LoginSiteBuilder {
            host: host.to_string(),
            members: 3,
            expire_after: None,
        }
    }

    pub fn host(&self) -> &str {
        &self.host
    }

    pub fn port(&self) -> u16 {
        self.addr.port()
    }

    pub fn origin(&self) -> String {
        format!("http://{}:{}", self.host, self.addr.port())
    }

    pub fn url(&self, path: &str) -> String {
        format!("{}{path}", self.origin())
    }

    pub fn resolve(&self) -> Vec<(String, SocketAddr)> {
        vec![(self.host.clone(), self.addr)]
    }

    /// Opens a session for `user` as a person's sign-in would and returns
    /// the `sid` cookie value, for a test to build a login session from.
    pub fn mint_session(&self, user: &str) -> String {
        new_session(&self.state, user)
    }

    /// Whether the server still holds a session with this `sid`.
    pub fn session_is_live(&self, sid: &str) -> bool {
        self.state
            .sessions
            .lock()
            .expect("sessions lock")
            .contains_key(sid)
    }

    pub fn hits(&self) -> Vec<LoginHit> {
        self.state.log.lock().expect("log lock").clone()
    }

    /// Requests for one path.
    pub fn count(&self, path: &str) -> usize {
        self.hits().iter().filter(|h| h.path == path).count()
    }

    /// Requests answered so far.
    pub fn request_count(&self) -> usize {
        self.state.requests.load(Ordering::SeqCst)
    }

    /// The request number after which the sessions were ended by
    /// `expire_after`, once that happened.
    pub fn expired_at(&self) -> Option<usize> {
        *self.state.expired_at.lock().expect("expired lock")
    }
}

fn new_session(state: &State, user: &str) -> String {
    let sid = random_token(&state.counter);
    state
        .sessions
        .lock()
        .expect("sessions lock")
        .insert(sid.clone(), user.to_string());
    sid
}

fn sid_of(cookie: &str) -> Option<&str> {
    cookie
        .split(';')
        .map(str::trim)
        .find_map(|c| c.strip_prefix("sid="))
}

fn user_param(s: &str) -> Option<String> {
    s.split('&')
        .find_map(|kv| kv.strip_prefix("user="))
        .filter(|u| !u.is_empty())
        .map(str::to_string)
}

fn handle(state: &State, req: &Request) -> Reply {
    let number = state.requests.fetch_add(1, Ordering::SeqCst) + 1;
    let path = req.path().to_string();
    let cookie = req.header("cookie").map(str::to_string);
    let sid = cookie.as_deref().and_then(sid_of).map(str::to_string);
    let user = sid
        .as_deref()
        .and_then(|s| state.sessions.lock().expect("sessions lock").get(s).cloned());
    state.log.lock().expect("log lock").push(LoginHit {
        path: path.clone(),
        cookie,
        signed_in: user.is_some(),
        number,
    });
    let reply = route(state, req, &path, sid.as_deref(), user.as_deref());
    if state.expire_after == Some(number) {
        state.sessions.lock().expect("sessions lock").clear();
        *state.expired_at.lock().expect("expired lock") = Some(number);
    }
    reply
}

fn page(title: &str, body: &str) -> Reply {
    Reply::html(
        200,
        &format!("<!doctype html><html><head><title>{title}</title></head><body>{body}</body></html>"),
    )
}

fn route(state: &State, req: &Request, path: &str, sid: Option<&str>, user: Option<&str>) -> Reply {
    match path {
        "/" => page(
            "Home",
            "<a href=\"/members/\">Members</a> <a href=\"/login\">Login</a>",
        ),
        "/login" => {
            let from = user_param(req.query().unwrap_or(""))
                .or_else(|| user_param(&String::from_utf8_lossy(&req.body)));
            match from {
                Some(name) => {
                    let sid = new_session(state, &name);
                    Reply::redirect(302, "/members/")
                        .header("Set-Cookie", format!("sid={sid}; Path=/; HttpOnly"))
                }
                None => page(
                    "Login",
                    "<form method=\"post\" action=\"/login\"><input name=\"user\"><button>Sign in</button></form>",
                ),
            }
        }
        "/logout" => {
            if let Some(sid) = sid {
                state.sessions.lock().expect("sessions lock").remove(sid);
            }
            Reply::redirect(302, "/login").header("Set-Cookie", "sid=; Path=/; Max-Age=0")
        }
        "/account" => match user {
            Some(name) => page(
                "Account",
                &format!("<p>Hello {name}</p><a href=\"/members/\">Members</a>"),
            ),
            None => page("Account", "<a href=\"/login\">Sign in</a>"),
        },
        "/members/" => {
            if user.is_none() {
                return Reply::redirect(302, "/login");
            }
            let mut links: String = (1..=state.members)
                .map(|i| format!("<a href=\"/members/{i}\">Member page {i}</a>\n"))
                .collect();
            links.push_str("<a href=\"/logout\">Sign out</a>\n");
            page("Members", &links)
        }
        _ => match path.strip_prefix("/members/") {
            Some(rest)
                if rest
                    .parse::<usize>()
                    .is_ok_and(|i| (1..=state.members).contains(&i)) =>
            {
                if user.is_none() {
                    return Reply::redirect(302, "/login");
                }
                page(
                    &format!("Member page {rest}"),
                    "<a href=\"/members/\">Members</a> <a href=\"/account\">Account</a>",
                )
            }
            _ => Reply::html(404, "<html><head><title>Not found</title></head></html>"),
        },
    }
}
