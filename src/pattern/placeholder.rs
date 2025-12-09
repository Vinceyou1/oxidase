use super::context::PatternContext;
use super::PatternError;
use crate::util::parse::parse_call;

/// Built-in placeholder types (parse-time)
#[derive(Debug, Clone)]
pub enum TypeSpec {
    Segment,                 // segment (changes by ctx)
    Slug,                    // [A-Za-z0-9_-]+
    Uint, Int, Hex, Alnum, Uuid,
    Path,                    // PathCtx only, tail-only
    Label, Labels,           // HostCtx only
    Any,                     // ValueCtx only
    Regex(String),           // in-segment
    RegexPath(String),       // PathCtx only, tail-only
    RegexLabels(String),     // HostCtx only
}

#[derive(Debug, Clone)]
pub struct Placeholder {
    pub name: Option<String>,
    pub ty: TypeSpec,
}

pub fn parse_placeholder<C: PatternContext>(buf: &str, ctx: &C) -> Result<Placeholder, PatternError> {
    let (lhs, ty_raw) = if let Some(colon) = buf.find(':') {
        (&buf[..colon], Some(&buf[colon + 1..]))
    } else { (buf, None) };

    let name = if lhs.is_empty() { None } else { Some(lhs.to_string()) };
    let ty = match ty_raw {
        None => ctx.default_type(),
        Some(t) => parse_type_spec(t, ctx)?,
    };

    Ok(Placeholder { name, ty })
}

pub fn parse_type_spec<C: PatternContext>(s: &str, ctx: &C) -> Result<TypeSpec, PatternError> {
    use TypeSpec::*;

    let s = s.trim();

    Ok(match s {
        "" => ctx.default_type(), "*" => ctx.asterisk_type(),
        "segment" => Segment, "slug" => Slug, "uint" => Uint, "int" => Int, "hex" => Hex, "alnum" => Alnum,
        "uuid" => Uuid, "path" => Path, "label" => Label, "labels" => Labels, "any" => Any,
        _ => {
            if let Ok((name, args)) = parse_call(s) {
                match (name.as_str(), args.as_slice()) {
                    ("regex", [arg]) => Regex(arg.clone()),
                    ("regex_path", [arg]) => RegexPath(arg.clone()),
                    ("regex_labels", [arg]) => RegexLabels(arg.clone()),
                    _ => return Err(PatternError::BadPlaceholder(s.into())),
                }
            } else {
                return Err(PatternError::BadPlaceholder(s.into()));
            }
        }
    })
}
