#[derive(Debug)]
pub enum ParseError {
    Invalid(String),
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseError::Invalid(s) => write!(f, "{s}"),
        }
    }
}

impl std::error::Error for ParseError {}

/// Parse a call-like string: "name" or "name(arg1,arg2)" with basic quoting/escaping.
pub fn parse_call(raw: &str) -> Result<(String, Vec<String>), ParseError> {
    let trimmed = raw.trim();
    if let Some(open) = trimmed.find('(') {
        let name = trimmed[..open].trim();
        if !trimmed.ends_with(')') {
            return Err(ParseError::Invalid("missing closing `)`".into()));
        }
        let inner = &trimmed[open + 1..trimmed.len() - 1];
        let args = split_args(inner)?;
        Ok((name.to_string(), args))
    } else {
        Ok((trimmed.to_string(), Vec::new()))
    }
}

/// Split comma-separated args with quotes and escapes.
pub fn split_args(inner: &str) -> Result<Vec<String>, ParseError> {
    let mut args = Vec::new();
    let mut buf = String::new();
    let mut chars = inner.chars().peekable();
    let mut in_quote: Option<char> = None;
    let mut esc = false;

    while let Some(ch) = chars.next() {
        if esc {
            buf.push(ch);
            esc = false;
            continue;
        }
        if ch == '\\' {
            esc = true;
            continue;
        }
        if let Some(q) = in_quote {
            if ch == q {
                in_quote = None;
                continue;
            }
            buf.push(ch);
            continue;
        }
        match ch {
            '\'' | '"' => {
                in_quote = Some(ch);
            }
            ',' => {
                args.push(buf.trim().to_string());
                buf.clear();
            }
            _ => buf.push(ch),
        }
    }

    if esc || in_quote.is_some() {
        return Err(ParseError::Invalid("unterminated escape or quote".into()));
    }

    if !buf.is_empty() || inner.ends_with(',') {
        args.push(buf.trim().to_string());
    }

    Ok(args)
}
