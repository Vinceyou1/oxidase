use std::collections::HashMap;

use bytes::Bytes;
use http_body_util::Full;
use hyper::{body, http};
use percent_encoding::percent_decode_str;

use crate::build::router::{
    CompiledBasicCond,
    CompiledCondNode,
    CompiledRouterMatch,
    CompiledTestCond,
    LoadedOp,
};
use crate::build::service::LoadedRouter;
use crate::config::http_method::HttpMethod;
use crate::config::router::OnMatch;
use crate::config::url_scheme::Scheme;
use crate::handler::{BoxResponseFuture, ServiceHandler};
use crate::template::{ValueProvider, expand_template};
use crate::util::http::make_error_resp;

#[derive(Debug, Clone)]
struct RouterCtx {
    method: Option<HttpMethod>,
    scheme: Option<String>,
    host: String,
    port: Option<u16>,
    path: String,
    query: HashMap<String, Vec<String>>,
    headers: HashMap<String, Vec<String>>,
    cookies: HashMap<String, String>,
    captures: HashMap<String, String>,
}

impl ValueProvider for RouterCtx {
    fn get(&self, key: &str) -> Option<String> {
        match key {
            "method" => self.method.as_ref().map(|m| format!("{:?}", m).to_ascii_uppercase()),
            "scheme" => self.scheme.clone(),
            "host" => Some(self.host.clone()),
            "port" => self.port.map(|p| p.to_string()),
            "path" => Some(self.path.clone()),
            v if v.starts_with("header.") => {
                let name = v.trim_start_matches("header.").to_ascii_lowercase();
                self.headers.get(&name).and_then(|vals| vals.get(0)).cloned()
            }
            v if v.starts_with("query.") => {
                let k = v.trim_start_matches("query.");
                self.query.get(k).and_then(|vals| vals.get(0)).cloned()
            }
            v if v.starts_with("cookie.") => {
                let k = v.trim_start_matches("cookie.");
                self.cookies.get(k).cloned()
            }
            _ => self.captures.get(key).cloned(),
        }
    }
}

impl RouterCtx {
    fn from_request(req: &http::Request<body::Incoming>) -> Self {
        let method = HttpMethod::try_from(req.method().as_str()).ok();
        let scheme = req.uri().scheme_str().map(|s| s.to_ascii_lowercase());
        let (host, port) = parse_host_and_port(req);
        let path = req.uri().path().to_string();
        let query = parse_query(req.uri().query());
        let headers = collect_headers(req);
        let cookies = parse_cookies(headers.get("cookie"));
        RouterCtx {
            method,
            scheme,
            host,
            port,
            path,
            query,
            headers,
            cookies,
            captures: HashMap::new(),
        }
    }
}

impl ServiceHandler for LoadedRouter {
    fn handle_request<'a>(
        &'a self,
        req: &'a mut http::Request<body::Incoming>,
    ) -> BoxResponseFuture<'a> {
        Box::pin(async move { route_request(self, req).await })
    }
}

async fn route_request(
    router: &LoadedRouter,
    req: &mut http::Request<body::Incoming>,
) -> http::Response<Full<Bytes>> {
    let mut ctx = RouterCtx::from_request(req);
    let mut step = 0u32;
    let mut idx = 0usize;

    loop {
        if step >= router.max_steps {
            return make_error_resp(http::StatusCode::LOOP_DETECTED, "router steps exceeded");
        }

        if idx >= router.rules.len() {
            if let Some(nx) = &router.next {
                apply_ctx_to_request(&ctx, req);
                return nx.handle_request(req).await;
            } else {
                return make_error_resp(http::StatusCode::NOT_FOUND, "no route matched");
            }
        }

        let rule = &router.rules[idx];

        match matches_rule(&rule.when, &mut ctx) {
            MatchResult::NoMatch => {
                idx += 1;
                continue;
            }
            MatchResult::Match => {}
        }

        match run_ops(&rule.ops, &mut ctx, req).await {
            OpOutcome::ContinueNextRule => {
                idx += 1;
            }
            OpOutcome::Restart => {
                step += 1;
                idx = 0;
            }
            OpOutcome::Respond(resp) => return resp,
            OpOutcome::UseService(resp) => return resp,
            OpOutcome::Fallthrough => {
                match rule.on_match {
                    OnMatch::Stop => {
                        if let Some(n) = &router.next {
                            apply_ctx_to_request(&ctx, req);
                            return n.handle_request(req).await;
                        } else {
                            return make_error_resp(http::StatusCode::NOT_FOUND, "no route matched");
                        }
                    }
                    OnMatch::Continue => idx += 1,
                    OnMatch::Restart => {
                        step += 1;
                        idx = 0;
                    }
                }
            }
        }
    }
}

#[derive(Debug)]
enum MatchResult {
    Match,
    NoMatch,
}

fn matches_rule(
    m: &CompiledRouterMatch,
    ctx: &mut RouterCtx,
) -> MatchResult {
    if let Some(host_pat) = &m.host {
        if !host_pat.is_match(&ctx.host) {
            return MatchResult::NoMatch;
        }
        if let Some(caps) = host_pat.captures_map(&ctx.host) {
            ctx.captures.extend(caps);
        }
    }

    if let Some(path_pat) = &m.path {
        if !path_pat.is_match(&ctx.path) {
            return MatchResult::NoMatch;
        }
        if let Some(caps) = path_pat.captures_map(&ctx.path) {
            ctx.captures.extend(caps);
        }
    }

    if let Some(scheme) = &m.scheme {
        let s = ctx.scheme.as_deref().unwrap_or("");
        let expect = match scheme {
            crate::config::router::r#match::Scheme::Http => "http",
            crate::config::router::r#match::Scheme::Https => "https",
        };
        if s != expect {
            return MatchResult::NoMatch;
        }
    }

    if !m.methods.is_empty() {
        if let Some(method) = &ctx.method {
            if !m.methods.iter().any(|mth| mth == method) {
                return MatchResult::NoMatch;
            }
        } else {
            return MatchResult::NoMatch;
        }
    }

    for h in &m.headers {
        let vals = ctx.headers.get(&h.name).cloned().unwrap_or_default();
        let matched = vals.iter().any(|v| h.pattern.is_match(v));
        let ok = if h.not { !matched } else { matched };
        if !ok {
            return MatchResult::NoMatch;
        }
        if let Some(v) = vals.first() {
            if let Some(caps) = h.pattern.captures_map(v) {
                ctx.captures.extend(caps);
            }
        }
    }

    for q in &m.queries {
        let vals = ctx.query.get(&q.key).cloned().unwrap_or_default();
        let matched = vals.iter().any(|v| q.pattern.is_match(v));
        let ok = if q.not { !matched } else { matched };
        if !ok {
            return MatchResult::NoMatch;
        }
        if let Some(v) = vals.first() {
            if let Some(caps) = q.pattern.captures_map(v) {
                ctx.captures.extend(caps);
            }
        }
    }

    for c in &m.cookies {
        let val = ctx.cookies.get(&c.name).cloned().unwrap_or_default();
        let matched = c.pattern.is_match(&val);
        let ok = if c.not { !matched } else { matched };
        if !ok {
            return MatchResult::NoMatch;
        }
        if let Some(caps) = c.pattern.captures_map(&val) {
            ctx.captures.extend(caps);
        }
    }

    MatchResult::Match
}

#[derive(Debug)]
enum OpOutcome {
    ContinueNextRule,
    Restart,
    Respond(http::Response<Full<Bytes>>),
    UseService(http::Response<Full<Bytes>>),
    Fallthrough,
}

async fn run_ops(
    ops: &[LoadedOp],
    ctx: &mut RouterCtx,
    req: &mut http::Request<body::Incoming>,
) -> OpOutcome {
    let mut stack: Vec<(&[LoadedOp], usize)> = vec![(ops, 0)];

    while let Some((ops_slice, mut idx)) = stack.pop() {
        while idx < ops_slice.len() {
            let op = &ops_slice[idx];
            match op {
                LoadedOp::SetScheme(s) => {
                    ctx.scheme = Some(match s {
                        Scheme::Http => "http".to_string(),
                        Scheme::Https => "https".to_string(),
                    });
                }
                LoadedOp::SetHost(tpl) => {
                    match expand_template(tpl, &ctx) {
                        Ok(val) => ctx.host = val,
                        Err(_) => return OpOutcome::Respond(make_error_resp(http::StatusCode::BAD_REQUEST, "template error")),
                    }
                }
                LoadedOp::SetPort(p) => ctx.port = Some(*p),
                LoadedOp::SetPath(tpl) => {
                        let val = match expand_template(tpl, &ctx) {
                        Ok(v) => v,
                        Err(_) => return OpOutcome::Respond(make_error_resp(http::StatusCode::BAD_REQUEST, "template error")),
                    };
                    if !val.starts_with('/') {
                        return OpOutcome::Respond(make_error_resp(http::StatusCode::BAD_REQUEST, "path must start with '/'"));
                    }
                    ctx.path = val;
                }
                LoadedOp::HeaderSet(map) => {
                    let headers = req.headers_mut();
                    for (k, v) in map {
                        let val = match expand_template(v, &ctx) {
                            Ok(v) => v,
                            Err(_) => return OpOutcome::Respond(make_error_resp(http::StatusCode::BAD_REQUEST, "template error")),
                        };
                        if let (Ok(name), Ok(hv)) = (
                            http::HeaderName::try_from(k.as_str()),
                            http::HeaderValue::from_str(&val),
                        ) {
                            headers.insert(name.clone(), hv);
                            ctx.headers.insert(name.as_str().to_ascii_lowercase(), vec![val]);
                        }
                    }
                }
                LoadedOp::HeaderAdd(map) => {
                    let headers = req.headers_mut();
                    for (k, v) in map {
                        let val = match expand_template(v, &ctx) {
                            Ok(v) => v,
                            Err(_) => return OpOutcome::Respond(make_error_resp(http::StatusCode::BAD_REQUEST, "template error")),
                        };
                        if let (Ok(name), Ok(hv)) = (
                            http::HeaderName::try_from(k.as_str()),
                            http::HeaderValue::from_str(&val),
                        ) {
                            headers.append(name.clone(), hv);
                            ctx.headers.entry(name.as_str().to_ascii_lowercase()).or_default().push(val);
                        }
                    }
                }
                LoadedOp::HeaderDelete(keys) => {
                    let headers = req.headers_mut();
                    for k in keys {
                        if let Ok(name) = http::HeaderName::try_from(k.as_str()) {
                            headers.remove(&name);
                            ctx.headers.remove(&name.as_str().to_ascii_lowercase());
                        }
                    }
                }
                LoadedOp::HeaderClear => {
                    req.headers_mut().clear();
                    ctx.headers.clear();
                }
                LoadedOp::QuerySet(map) => {
                    for (k, v) in map {
                        let val = match expand_template(v, &ctx) {
                            Ok(v) => v,
                            Err(_) => return OpOutcome::Respond(make_error_resp(http::StatusCode::BAD_REQUEST, "template error")),
                        };
                        ctx.query.insert(k.clone(), vec![val]);
                    }
                }
                LoadedOp::QueryAdd(map) => {
                    for (k, v) in map {
                        let val = match expand_template(v, &ctx) {
                            Ok(v) => v,
                            Err(_) => return OpOutcome::Respond(make_error_resp(http::StatusCode::BAD_REQUEST, "template error")),
                        };
                        ctx.query.entry(k.clone()).or_default().push(val);
                    }
                }
                LoadedOp::QueryDelete(keys) => {
                    for k in keys {
                        ctx.query.remove(k);
                    }
                }
                LoadedOp::QueryClear => ctx.query.clear(),
                LoadedOp::InternalRewrite => return OpOutcome::Restart,
                LoadedOp::Redirect { status, location } => {
                    let status_code = match status {
                        crate::config::router::op::RedirectCode::_301 => http::StatusCode::MOVED_PERMANENTLY,
                        crate::config::router::op::RedirectCode::_302 => http::StatusCode::FOUND,
                        crate::config::router::op::RedirectCode::_307 => http::StatusCode::TEMPORARY_REDIRECT,
                        crate::config::router::op::RedirectCode::_308 => http::StatusCode::PERMANENT_REDIRECT,
                    };
                    let loc = match expand_template(location, &ctx) {
                        Ok(v) => v,
                        Err(_) => return OpOutcome::Respond(make_error_resp(http::StatusCode::BAD_REQUEST, "template error")),
                    };
                    let resp = http::Response::builder()
                        .status(status_code)
                        .header(http::header::LOCATION, loc.as_str())
                        .body(Full::default())
                        .unwrap_or_else(|_| make_error_resp(http::StatusCode::INTERNAL_SERVER_ERROR, "redirect build failed"));
                    return OpOutcome::Respond(resp);
                }
                LoadedOp::Respond { status, body, headers } => {
                    let mut builder = http::Response::builder().status(*status);
                    for (k, v) in headers {
                        let val = match expand_template(v, &ctx) {
                            Ok(v) => v,
                            Err(_) => return OpOutcome::Respond(make_error_resp(http::StatusCode::BAD_REQUEST, "template error")),
                        };
                        if let (Ok(name), Ok(val)) = (
                            http::HeaderName::try_from(k.as_str()),
                            http::HeaderValue::from_str(&val),
                        ) {
                            builder = builder.header(name, val);
                        }
                    }
                    let body_val = match body {
                        Some(t) => match expand_template(t, &ctx) {
                            Ok(v) => v,
                            Err(_) => return OpOutcome::Respond(make_error_resp(http::StatusCode::BAD_REQUEST, "template error")),
                        },
                        None => String::new(),
                    };
                    let resp = builder
                        .body(Full::from(body_val))
                        .unwrap_or_else(|_| make_error_resp(http::StatusCode::INTERNAL_SERVER_ERROR, "respond build failed"));
                    return OpOutcome::Respond(resp);
                }
                LoadedOp::Use(svc) => {
                    apply_ctx_to_request(ctx, req);
                    let resp = svc.handle_request(req).await;
                    return OpOutcome::UseService(resp);
                }
                LoadedOp::Branch(cond, then_ops, else_ops) => {
                    let pass = eval_cond(cond, ctx);
                    let ops_to_run = if pass { then_ops } else { else_ops };
                    stack.push((ops_slice, idx + 1));
                    stack.push((ops_to_run, 0));
                    break;
                }
            }
            idx += 1;
        }
    }

    OpOutcome::Fallthrough
}

fn eval_cond(node: &CompiledCondNode, ctx: &RouterCtx) -> bool {
    match node {
        CompiledCondNode::All(children) => children.iter().all(|n| eval_cond(n, ctx)),
        CompiledCondNode::Any(children) => children.iter().any(|n| eval_cond(n, ctx)),
        CompiledCondNode::Not(child) => !eval_cond(child, ctx),
        CompiledCondNode::Test(t) => eval_test(t, ctx),
    }
}

fn eval_test(t: &CompiledTestCond, ctx: &RouterCtx) -> bool {
    match &t.cond {
        CompiledBasicCond::Equals(is) => {
            value_of(&t.var, ctx).map_or(false, |v| serde_yaml::Value::String(v) == *is)
        }
        CompiledBasicCond::In(list) => {
            value_of(&t.var, ctx).map_or(false, |v| list.contains(&serde_yaml::Value::String(v)))
        }
        CompiledBasicCond::Present(p) => {
            let has = value_of(&t.var, ctx).is_some();
            has == *p
        }
        CompiledBasicCond::Pattern(pat) => {
            value_of(&t.var, ctx).map_or(false, |v| pat.is_match(&v))
        }
    }
}

fn value_of(var: &str, ctx: &RouterCtx) -> Option<String> {
    match var {
        "method" => ctx.method.as_ref().map(|m| format!("{:?}", m).to_ascii_uppercase()),
        "scheme" => ctx.scheme.clone(),
        "host" => Some(ctx.host.clone()),
        "port" => ctx.port.map(|p| p.to_string()),
        "path" => Some(ctx.path.clone()),
        v if v.starts_with("header.") => {
            let key = v.trim_start_matches("header.").to_ascii_lowercase();
            ctx.headers.get(&key).and_then(|vals| vals.get(0)).cloned()
        }
        v if v.starts_with("query.") => {
            let key = v.trim_start_matches("query.");
            ctx.query.get(key).and_then(|vals| vals.get(0)).cloned()
        }
        v if v.starts_with("cookie.") => {
            let key = v.trim_start_matches("cookie.");
            ctx.cookies.get(key).cloned()
        }
        _ => ctx.captures.get(var).cloned(),
    }
}

fn apply_ctx_to_request(ctx: &RouterCtx, req: &mut http::Request<body::Incoming>) {
    if !ctx.host.is_empty() {
        if let Ok(val) = http::HeaderValue::from_str(&ctx.host) {
            req.headers_mut().insert(http::header::HOST, val);
        }
    }

    let mut uri = ctx.path.clone();
    if !ctx.query.is_empty() {
        let mut parts = Vec::new();
        for (k, vals) in &ctx.query {
            for v in vals {
                parts.push(format!("{k}={v}"));
            }
        }
        uri.push('?');
        uri.push_str(&parts.join("&"));
    }
    if let Ok(new_uri) = uri.parse() {
        *req.uri_mut() = new_uri;
    }
}

fn parse_host_and_port(req: &http::Request<body::Incoming>) -> (String, Option<u16>) {
    if let Some(host) = req.uri().host() {
        let port = req.uri().port_u16();
        return (host.to_string(), port);
    }
    if let Some(host_header) = req.headers().get(http::header::HOST) {
        if let Ok(hs) = host_header.to_str() {
            if let Some((h, p)) = hs.split_once(':') {
                if let Ok(port) = p.parse::<u16>() {
                    return (h.to_string(), Some(port));
                }
            }
            return (hs.to_string(), None);
        }
    }
    ("".into(), None)
}

fn parse_query(q: Option<&str>) -> HashMap<String, Vec<String>> {
    let mut out: HashMap<String, Vec<String>> = HashMap::new();
    if let Some(qs) = q {
        for pair in qs.split('&') {
            if pair.is_empty() { continue; }
            let mut iter = pair.splitn(2, '=');
            let key = iter.next().unwrap_or("").to_string();
            let val = iter.next().unwrap_or("").to_string();
            out.entry(key).or_default().push(val);
        }
    }
    out
}

fn collect_headers(req: &http::Request<body::Incoming>) -> HashMap<String, Vec<String>> {
    let mut map: HashMap<String, Vec<String>> = HashMap::new();
    for (name, value) in req.headers() {
        let key = name.as_str().to_ascii_lowercase();
        if let Ok(vs) = value.to_str() {
            map.entry(key).or_default().push(vs.to_string());
        }
    }
    map
}

fn parse_cookies(cookies: Option<&Vec<String>>) -> HashMap<String, String> {
    let mut out = HashMap::new();
    if let Some(list) = cookies {
        for raw in list {
            for part in raw.split(';') {
                let trimmed = part.trim();
                if trimmed.is_empty() { continue; }
                if let Some((k, v)) = trimmed.split_once('=') {
                    let key = k.trim();
                    let val = percent_decode_str(v.trim()).decode_utf8_lossy().to_string();
                    out.insert(key.to_string(), val);
                }
            }
        }
    }
    out
}

impl TryFrom<&str> for HttpMethod {
    type Error = ();

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "GET" => Ok(HttpMethod::Get),
            "POST" => Ok(HttpMethod::Post),
            "PUT" => Ok(HttpMethod::Put),
            "PATCH" => Ok(HttpMethod::Patch),
            "DELETE" => Ok(HttpMethod::Delete),
            "HEAD" => Ok(HttpMethod::Head),
            "OPTIONS" => Ok(HttpMethod::Options),
            _ => Err(()),
        }
    }
}
