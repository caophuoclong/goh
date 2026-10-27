//! Minimal shell-word tokenizer for the text before the cursor.
//!
//! No glob, variable or command substitution. Anything we do not understand
//! (unterminated quote, trailing backslash) yields `None` so the engine stays silent.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    /// Byte span in the buffer.
    pub start: usize,
    pub end: usize,
    /// Unquoted, unescaped value.
    pub value: String,
    pub quoted: bool,
}

#[derive(Debug)]
pub struct Parsed {
    /// Completed words of the current simple command.
    pub words: Vec<Token>,
    /// The word under the cursor (empty when the prefix ends in whitespace).
    pub current: Token,
}

pub fn parse(prefix: &str) -> Option<Parsed> {
    let mut words: Vec<Token> = Vec::new();
    let mut cur: Option<Token> = None;
    let mut quote: Option<char> = None;
    let mut escaped = false;

    let finish = |words: &mut Vec<Token>, cur: &mut Option<Token>, end: usize| {
        if let Some(mut t) = cur.take() {
            t.end = end;
            let separator = !t.quoted && matches!(t.value.as_str(), "|" | "||" | "&&" | ";");
            if separator {
                words.clear();
            } else {
                words.push(t);
            }
        }
    };

    for (i, c) in prefix.char_indices() {
        if !escaped && quote.is_none() && c.is_whitespace() {
            finish(&mut words, &mut cur, i);
            continue;
        }
        let word = cur.get_or_insert_with(|| Token { start: i, end: i, value: String::new(), quoted: false });
        if escaped {
            word.value.push(c);
            escaped = false;
        } else if let Some(q) = quote {
            if c == q {
                quote = None;
            } else if c == '\\' && q == '"' {
                escaped = true;
            } else {
                word.value.push(c);
            }
        } else if c == '\\' {
            escaped = true;
        } else if c == '\'' || c == '"' {
            quote = Some(c);
            word.quoted = true;
        } else {
            word.value.push(c);
        }
    }

    if quote.is_some() || escaped {
        return None;
    }
    let current = cur.map(|mut t| {
        t.end = prefix.len();
        t
    });
    Some(Parsed {
        words,
        current: current.unwrap_or(Token { start: prefix.len(), end: prefix.len(), value: String::new(), quoted: false }),
    })
}

/// Length in bytes of the rest of the word that continues after the cursor.
pub fn word_tail(suffix: &str) -> usize {
    let mut escaped = false;
    for (i, c) in suffix.char_indices() {
        if escaped {
            escaped = false;
        } else if c == '\\' {
            escaped = true;
        } else if c.is_whitespace() {
            return i;
        }
    }
    suffix.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(p: &Parsed) -> Vec<&str> {
        p.words.iter().map(|t| t.value.as_str()).collect()
    }

    #[test]
    fn splits_words_and_current() {
        let p = parse("git ch").unwrap();
        assert_eq!(words(&p), ["git"]);
        assert_eq!((p.current.value.as_str(), p.current.start, p.current.end), ("ch", 4, 6));
    }

    #[test]
    fn trailing_space_gives_empty_current_at_cursor() {
        let p = parse("git ").unwrap();
        assert_eq!(words(&p), ["git"]);
        assert_eq!((p.current.start, p.current.end), (4, 4));
    }

    #[test]
    fn escaped_space_stays_in_token() {
        let p = parse(r"cat My\ Pro").unwrap();
        assert_eq!(p.current.value, "My Pro");
        assert_eq!(p.current.start, 4);
    }

    #[test]
    fn unterminated_quote_or_backslash_is_conservative() {
        assert!(parse("echo \"hi").is_none());
        assert!(parse("echo hi\\").is_none());
    }

    #[test]
    fn pipeline_starts_new_command() {
        let p = parse("ls | gi").unwrap();
        assert!(p.words.is_empty());
        assert_eq!(p.current.value, "gi");
    }

    #[test]
    fn tail_respects_escapes() {
        assert_eq!(word_tail(r"jects rest"), 5);
        assert_eq!(word_tail(r"a\ b c"), 4);
    }
}
