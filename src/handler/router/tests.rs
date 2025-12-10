use std::collections::HashMap;

use crate::pattern::{compile, compile_host};
use crate::pattern::context::PathCtx;
use crate::template::{compile_template, expand_template, CompiledTemplate, ValueProvider};

use super::ctx::RouterCtx;
use super::ops::eval_cond;
use crate::build::router::{CompiledBasicCond, CompiledCondNode, CompiledTestCond};

fn ctx_with_path(path: &str) -> RouterCtx {
    RouterCtx {
        method: None,
        scheme: None,
        host: String::new(),
        port: None,
        path: path.to_string(),
        query: HashMap::new(),
        headers: HashMap::new(),
        cookies: HashMap::new(),
        captures: HashMap::new(),
    }
}

fn ctx_with_host(host: &str) -> RouterCtx {
    RouterCtx {
        method: None,
        scheme: None,
        host: host.to_string(),
        port: None,
        path: String::new(),
        query: HashMap::new(),
        headers: HashMap::new(),
        cookies: HashMap::new(),
        captures: HashMap::new(),
    }
}

fn test_node<C: crate::pattern::context::PatternContext>(
    var: &str,
    pat: &str,
    ctx: &C,
) -> CompiledCondNode {
    let pattern = compile(pat, ctx).unwrap();
    CompiledCondNode::Test(CompiledTestCond {
        var: var.to_string(),
        cond: CompiledBasicCond::Pattern(pattern),
    })
}

// --- captures tests ---

#[test]
fn any_takes_first_true_captures() {
    let cond = CompiledCondNode::Any(vec![
        test_node("path", "<id:uint>", &PathCtx),
        test_node("path", "<other:path>", &PathCtx),
    ]);
    let ctx = ctx_with_path("123");
    let (pass, caps) = eval_cond(&cond, &ctx);
    assert!(pass);
    assert_eq!(caps.get("id").map(String::as_str), Some("123"));
    assert!(!caps.contains_key("other"));
}

#[test]
fn all_merges_captures() {
    let cond = CompiledCondNode::All(vec![
        test_node("path", "<a:uint>", &PathCtx),
        test_node("path", "<b:regex([0-9]{3})>", &PathCtx),
    ]);
    let ctx = ctx_with_path("123");
    let (pass, caps) = eval_cond(&cond, &ctx);
    assert!(pass);
    assert_eq!(caps.get("a").map(String::as_str), Some("123"));
    assert_eq!(caps.get("b").map(String::as_str), Some("123"));
}

#[test]
fn not_does_not_propagate_captures() {
    let cond = CompiledCondNode::Not(Box::new(test_node("path", "<p:*>", &PathCtx)));
    let ctx = ctx_with_path("/whatever");
    let (pass, caps) = eval_cond(&cond, &ctx);
    assert!(!pass);
    assert!(caps.is_empty());
}

#[test]
fn any_all_fail_no_captures() {
    let cond = CompiledCondNode::Any(vec![
        test_node("path", "<id:uint>", &PathCtx),
        test_node("path", "<slug:slug>", &PathCtx),
    ]);
    let ctx = ctx_with_path("bad.slug"); // contains '.' so neither uint nor slug matches
    let (pass, caps) = eval_cond(&cond, &ctx);
    assert!(!pass);
    assert!(caps.is_empty());
}

#[test]
fn all_stops_on_first_fail_and_drops_captures() {
    let cond = CompiledCondNode::All(vec![
        test_node("path", "<id:uint>", &PathCtx),
        test_node("path", "<never:uint>", &PathCtx), // will fail because path already consumed non-digit?
    ]);
    let ctx = ctx_with_path("abc");
    let (pass, caps) = eval_cond(&cond, &ctx);
    assert!(!pass);
    assert!(caps.is_empty());
}

#[test]
fn any_inside_all_merges_chosen_branch_only() {
    let cond = CompiledCondNode::All(vec![
        test_node("path", "<p:uint>", &PathCtx),
        CompiledCondNode::Any(vec![
            test_node("path", "<x:uint>", &PathCtx),
            test_node("path", "<y:slug>", &PathCtx),
        ]),
    ]);
    let ctx = ctx_with_path("123");
    let (pass, caps) = eval_cond(&cond, &ctx);
    assert!(pass);
    assert_eq!(caps.get("p").map(String::as_str), Some("123"));
    assert_eq!(caps.get("x").map(String::as_str), Some("123"));
    assert!(!caps.contains_key("y"));
}

#[test]
fn captures_with_same_key_follow_last_writer() {
    let host_node = CompiledCondNode::Test(CompiledTestCond {
        var: "host".to_string(),
        cond: CompiledBasicCond::Pattern(compile_host("<id:label>.example.com").unwrap()),
    });
    let path_node = test_node("path", "<id:uint>", &PathCtx);
    let cond = CompiledCondNode::All(vec![
        host_node,
        path_node,
    ]);
    let mut ctx = ctx_with_host("api.example.com");
    ctx.path = "123".to_string();
    let (pass, caps) = eval_cond(&cond, &ctx);
    assert!(pass);
    // path capture should overwrite host capture because it runs later
    assert_eq!(caps.get("id").map(String::as_str), Some("123"));
}

#[test]
fn host_pattern_context_compiles_and_captures() {
    let pattern = compile_host("<sub:label>.example.com").unwrap();
    let cond = CompiledCondNode::Test(CompiledTestCond {
        var: "host".to_string(),
        cond: CompiledBasicCond::Pattern(pattern),
    });
    let ctx = ctx_with_host("api.example.com");
    let (pass, caps) = eval_cond(&cond, &ctx);
    assert!(pass);
    assert_eq!(caps.get("sub").map(String::as_str), Some("api"));
}

#[test]
fn equals_and_present_do_not_capture() {
    let cond = CompiledCondNode::All(vec![
        CompiledCondNode::Test(CompiledTestCond {
            var: "path".to_string(),
            cond: CompiledBasicCond::Equals(serde_yaml::Value::String("/foo".into())),
        }),
        CompiledCondNode::Test(CompiledTestCond {
            var: "path".to_string(),
            cond: CompiledBasicCond::Present(true),
        }),
    ]);
    let ctx = ctx_with_path("/foo");
    let (pass, caps) = eval_cond(&cond, &ctx);
    assert!(pass);
    assert!(caps.is_empty());
}

// --- template tests ---

#[derive(Default)]
struct DummyProvider {
    map: std::collections::HashMap<String, String>,
}

impl ValueProvider for DummyProvider {
    fn get(&self, key: &str) -> Option<String> {
        self.map.get(key).cloned()
    }
}

fn tpl(src: &str) -> CompiledTemplate {
    compile_template(src).unwrap()
}

fn expand(tpl: &CompiledTemplate, map: &[(&str, &str)]) -> String {
    let mut prov = DummyProvider::default();
    for (k, v) in map {
        prov.map.insert(k.to_string(), v.to_string());
    }
    expand_template(tpl, &prov).unwrap()
}

#[test]
fn template_vars_and_literals() {
    let t = tpl("hello ${name}!");
    let out = expand(&t, &[("name", "world")]);
    assert_eq!(out, "hello world!");
}

#[test]
fn template_missing_var_defaults_to_empty() {
    let t = tpl("x${missing}y");
    let out = expand(&t, &[]);
    assert_eq!(out, "xy");
}

#[test]
fn template_filters_chain_in_order() {
    let t = tpl("${val | trim_prefix(\"pre-\") | replace(\"-\", \"_\") | upper}");
    let out = expand(&t, &[("val", "pre-ab-c")]);
    assert_eq!(out, "AB_C");
}

#[test]
fn template_default_only_when_empty() {
    let t = tpl("${val | default(\"zzz\")}");
    assert_eq!(expand(&t, &[("val", "")]), "zzz");
    assert_eq!(expand(&t, &[("val", "ok")]), "ok");
}

#[test]
fn template_url_encode_specials() {
    let t = tpl("${p | url_encode}");
    let out = expand(&t, &[("p", "a b/c?汉")]);
    assert_eq!(out, "a%20b%2Fc%3F%E6%B1%89");
}

#[test]
fn template_header_and_query_case_insensitive() {
    let mut ctx = RouterCtx {
        method: None,
        scheme: None,
        host: String::new(),
        port: None,
        path: String::new(),
        query: HashMap::new(),
        headers: HashMap::new(),
        cookies: HashMap::new(),
        captures: HashMap::new(),
    };
    ctx.headers.insert("x-foo".into(), vec!["Bar".into()]);
    ctx.query.insert("q".into(), vec!["1".into()]);
    let t = tpl("h=${header.X-Foo},q=${query.q}");
    let out = expand_template(&t, &ctx).unwrap();
    assert_eq!(out, "h=Bar,q=1");
}

#[test]
fn template_capture_overwrites() {
    let mut ctx = ctx_with_path("/foo");
    ctx.captures.insert("id".into(), "111".into());
    ctx.captures.insert("id".into(), "222".into());
    let t = tpl("${id}");
    let out = expand_template(&t, &ctx).unwrap();
    assert_eq!(out, "222");
}
