#[derive(Debug, Clone)]
pub enum Filter {
    Default(String),
    Lower,
    Upper,
    UrlEncode,
    TrimPrefix(String),
    TrimSuffix(String),
    Replace { from: String, to: String },
}

pub struct FilterSpec {
    pub name: &'static str,
    pub arity: usize,
}

pub const FILTER_SPECS: &[FilterSpec] = &[
    FilterSpec { name: "lower", arity: 0 },
    FilterSpec { name: "upper", arity: 0 },
    FilterSpec { name: "url_encode", arity: 0 },
    FilterSpec { name: "default", arity: 1 },
    FilterSpec { name: "trim_prefix", arity: 1 },
    FilterSpec { name: "trim_suffix", arity: 1 },
    FilterSpec { name: "replace", arity: 2 },
];

pub fn build_filter(name: &str, args: &[String]) -> Option<Filter> {
    match name {
        "lower" => Some(Filter::Lower),
        "upper" => Some(Filter::Upper),
        "url_encode" => Some(Filter::UrlEncode),
        "default" => args.get(0).map(|v| Filter::Default(v.clone())),
        "trim_prefix" => args.get(0).map(|v| Filter::TrimPrefix(v.clone())),
        "trim_suffix" => args.get(0).map(|v| Filter::TrimSuffix(v.clone())),
        "replace" => {
            if args.len() == 2 {
                Some(Filter::Replace { from: args[0].clone(), to: args[1].clone() })
            } else { None }
        }
        _ => None,
    }
}
