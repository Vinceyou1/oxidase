mod filter;

use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
pub use filter::{Filter, FILTER_SPECS, build_filter};
use crate::util::parse::parse_call;

#[derive(Debug, Clone)]
pub enum TemplateSegment {
    Literal(String),
    Expr { var: String, filters: Vec<Filter> },
}

#[derive(Debug, Clone)]
pub struct CompiledTemplate {
    segments: Vec<TemplateSegment>,
}

#[derive(Debug)]
pub enum TemplateError {
    Invalid(String),
}

impl std::fmt::Display for TemplateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TemplateError::Invalid(s) => write!(f, "template error: {s}"),
        }
    }
}

impl std::error::Error for TemplateError {}

pub trait ValueProvider {
    fn get(&self, key: &str) -> Option<String>;
}

impl<T: ValueProvider + ?Sized> ValueProvider for &T {
    fn get(&self, key: &str) -> Option<String> { (*self).get(key) }
}

impl<T: ValueProvider + ?Sized> ValueProvider for &mut T {
    fn get(&self, key: &str) -> Option<String> { (**self).get(key) }
}

pub fn compile_template(src: &str) -> Result<CompiledTemplate, TemplateError> {
    let mut segments = Vec::new();
    let mut buf = String::new();
    let mut chars = src.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '$' && chars.peek() == Some(&'{') {
            if !buf.is_empty() {
                segments.push(TemplateSegment::Literal(std::mem::take(&mut buf)));
            }
            chars.next(); // consume '{'
            let mut expr = String::new();
            let mut depth = 1;
            while let Some(c) = chars.next() {
                if c == '{' {
                    depth += 1;
                } else if c == '}' {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                expr.push(c);
            }
            if depth != 0 {
                return Err(TemplateError::Invalid("unclosed `${`".to_string()));
            }
            segments.push(TemplateSegment::Expr {
                var: parse_var(&expr)?,
                filters: parse_filters(&expr)?,
            });
        } else {
            buf.push(ch);
        }
    }

    if !buf.is_empty() {
        segments.push(TemplateSegment::Literal(buf));
    }

    Ok(CompiledTemplate { segments })
}

pub fn expand_template<T: ValueProvider>(
    tpl: &CompiledTemplate,
    provider: &T,
) -> Result<String, TemplateError> {
    let mut out = String::new();
    for seg in &tpl.segments {
        match seg {
            TemplateSegment::Literal(s) => out.push_str(s),
            TemplateSegment::Expr { var, filters } => {
                let mut val = provider.get(var).unwrap_or_default();
                for f in filters {
                    val = apply_filter(f, val);
                }
                out.push_str(&val);
            }
        }
    }
    Ok(out)
}

fn parse_var(expr: &str) -> Result<String, TemplateError> {
    let var = expr.split('|').next().unwrap_or("").trim();
    if var.is_empty() {
        return Err(TemplateError::Invalid("empty variable".to_string()));
    }
    Ok(var.to_string())
}

fn parse_filters(expr: &str) -> Result<Vec<Filter>, TemplateError> {
    let mut filters = Vec::new();
    let mut parts = expr.split('|');
    parts.next(); // skip var
    for raw in parts {
        let raw = raw.trim();
        if raw.is_empty() { continue; }
        let (name, args) = parse_call(raw).map_err(|e| TemplateError::Invalid(e.to_string()))?;
        let name_str = name.as_str();
        let arity = FILTER_SPECS.iter().find(|spec| spec.name == name_str).map(|spec| spec.arity);
        let filt = match arity {
            Some(n) if args.len() == n => build_filter(name_str, &args),
            _ => None,
        }.ok_or_else(|| TemplateError::Invalid(format!("unknown filter or args: {raw}")))?;
        filters.push(filt);
    }
    Ok(filters)
}

fn apply_filter(f: &Filter, val: String) -> String {
    match f {
        Filter::Default(v) => if val.is_empty() { v.clone() } else { val },
        Filter::Lower => val.to_lowercase(),
        Filter::Upper => val.to_uppercase(),
        Filter::UrlEncode => utf8_percent_encode(&val, NON_ALPHANUMERIC).to_string(),
        Filter::TrimPrefix(p) => val.strip_prefix(p).unwrap_or(&val).to_string(),
        Filter::TrimSuffix(p) => val.strip_suffix(p).unwrap_or(&val).to_string(),
        Filter::Replace { from, to } => val.replace(from, to),
    }
}

#[cfg(test)]
mod tests;
