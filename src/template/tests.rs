use super::*;

#[derive(Default)]
struct MapProvider(std::collections::HashMap<String, String>);
impl ValueProvider for MapProvider {
    fn get(&self, key: &str) -> Option<String> { self.0.get(key).cloned() }
}

#[test]
fn compile_and_expand_with_filters() {
    let tpl = compile_template("hi ${name|upper}, ${v|default(\"x\")}!").unwrap();
    let mut m = std::collections::HashMap::new();
    m.insert("name".into(), "bob".into());
    let ctx = MapProvider(m);
    let out = expand_template(&tpl, &ctx).unwrap();
    assert_eq!(out, "hi BOB, x!");
}

#[test]
fn parse_filters_with_args() {
    let tpl = compile_template("${slug|trim_prefix(\"/api/\")|replace(\"/\", \"-\")}").unwrap();
    let mut m = std::collections::HashMap::new();
    m.insert("slug".into(), "/api/v1/users".into());
    let ctx = MapProvider(m);
    let out = expand_template(&tpl, &ctx).unwrap();
    assert_eq!(out, "v1-users");
}
