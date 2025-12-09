use crate::config::error::ConfigError;
use crate::config::http_method::HttpMethod;
use crate::config::pattern::{
    compile_host,
    compile_path,
    compile_value,
    CompiledPattern,
};
use crate::config::router::r#match::{
    RouterMatch,
    HeaderCond,
    QueryCond,
    CookieCond,
    Scheme as RouterScheme,
};
use crate::config::router::op::RouterOp;
use crate::config::router::{RouterRule, OnMatch};

#[derive(Debug, Clone)]
pub struct LoadedRule {
    pub when: CompiledRouterMatch,
    pub ops: Vec<RouterOp>,
    pub on_match: OnMatch,
}

#[derive(Debug, Clone)]
pub struct CompiledRouterMatch {
    pub host: Option<CompiledPattern>,
    pub path: Option<CompiledPattern>,
    pub methods: Vec<HttpMethod>,
    pub headers: Vec<CompiledHeaderCond>,
    pub queries: Vec<CompiledQueryCond>,
    pub cookies: Vec<CompiledCookieCond>,
    pub scheme: Option<RouterScheme>,
}

#[derive(Debug, Clone)]
pub struct CompiledHeaderCond {
    pub name: String,
    pub pattern: CompiledPattern,
    pub not: bool,
}

#[derive(Debug, Clone)]
pub struct CompiledQueryCond {
    pub key: String,
    pub pattern: CompiledPattern,
    pub not: bool,
}

#[derive(Debug, Clone)]
pub struct CompiledCookieCond {
    pub name: String,
    pub pattern: CompiledPattern,
    pub not: bool,
}

pub fn compile_rules(rules: &[RouterRule]) -> Result<Vec<LoadedRule>, ConfigError> {
    rules.iter().map(compile_rule).collect()
}

fn compile_rule(rule: &RouterRule) -> Result<LoadedRule, ConfigError> {
    Ok(LoadedRule {
        when: compile_match(&rule.when)?,
        ops: rule.ops.clone(),
        on_match: rule.on_match.clone(),
    })
}

fn compile_match(m: &RouterMatch) -> Result<CompiledRouterMatch, ConfigError> {
    Ok(CompiledRouterMatch {
        host: compile_opt_pattern(m.host.as_deref(), compile_host)?,
        path: compile_opt_pattern(m.path.as_deref(), compile_path)?,
        methods: m.methods.clone(),
        headers: compile_headers(&m.headers)?,
        queries: compile_queries(&m.queries)?,
        cookies: compile_cookies(&m.cookies)?,
        scheme: m.scheme.clone(),
    })
}

fn compile_headers(headers: &[HeaderCond]) -> Result<Vec<CompiledHeaderCond>, ConfigError> {
    headers.iter().map(|hc| {
        Ok(CompiledHeaderCond {
            name: hc.name.to_ascii_lowercase(),
            pattern: compile_value(&hc.pattern).map_err(to_config_err)?,
            not: hc.not,
        })
    }).collect()
}

fn compile_queries(queries: &[QueryCond]) -> Result<Vec<CompiledQueryCond>, ConfigError> {
    queries.iter().map(|qc| {
        Ok(CompiledQueryCond {
            key: qc.key.clone(),
            pattern: compile_value(&qc.pattern).map_err(to_config_err)?,
            not: qc.not,
        })
    }).collect()
}

fn compile_cookies(cookies: &[CookieCond]) -> Result<Vec<CompiledCookieCond>, ConfigError> {
    cookies.iter().map(|cc| {
        Ok(CompiledCookieCond {
            name: cc.name.clone(),
            pattern: compile_value(&cc.pattern).map_err(to_config_err)?,
            not: cc.not,
        })
    }).collect()
}

fn compile_opt_pattern<F>(
    input: Option<&str>,
    f: F,
) -> Result<Option<CompiledPattern>, ConfigError>
where
    F: Fn(&str) -> Result<CompiledPattern, crate::config::pattern::error::PatternError>
{
    input.map(|s| f(s).map_err(to_config_err)).transpose()
}

fn to_config_err<E: std::error::Error>(e: E) -> ConfigError {
    ConfigError::Invalid(e.to_string())
}
