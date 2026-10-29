//! What the user's shell knows that the specs do not: aliases, functions, builtins and the commands on
//! `PATH`. The host fills it in; the engine uses it to see through an alias (`g checkout` completes as
//! `git checkout`) and to complete the command word itself.

use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameKind {
    Alias,
    Function,
    Builtin,
    Executable,
}

#[derive(Debug, Clone, Default)]
pub struct ShellContext {
    pub aliases: HashMap<String, String>,
    pub names: Vec<(String, NameKind)>,
    /// Names (never values) of the exported variables, for `$NA` and `export NA`.
    pub env: Vec<String>,
    /// The absolute folders of `$CDPATH`, where `cd name` also looks.
    pub cdpath: Vec<String>,
}

/// A line with its first word (an alias) replaced by what it stands for.
#[derive(Debug, PartialEq, Eq)]
pub struct Expanded {
    pub buffer: String,
    pub cursor: usize,
    /// Bytes the line grew by (negative when the expansion is shorter): spans in the expanded line map back by subtracting it.
    pub delta: isize,
}

const MAX_DEPTH: usize = 5;

/// Only plain words are expanded: quotes, pipes, substitutions and the like would need a shell to read.
fn simple(value: &str) -> bool {
    !value.is_empty() && !value.chars().any(|c| c.is_control() || "\\'\"`$;|&<>(){}*?~#!".contains(c))
}

impl ShellContext {
    /// Expand the first word when the cursor is past it (so it is complete) and it is an alias.
    pub fn expand(&self, buffer: &str, cursor: usize) -> Option<Expanded> {
        let first_end = buffer.find(char::is_whitespace)?;
        if first_end == 0 || cursor <= first_end {
            return None;
        }
        // Replace the leading word by its alias until it is not one: `gs` -> `g status` -> `git status`.
        let original = buffer[..first_end].to_string();
        let mut head = original.clone();
        let mut seen: Vec<String> = Vec::new();
        for _ in 0..MAX_DEPTH {
            let first = head.split_whitespace().next().unwrap_or("").to_string();
            let Some(value) = self.aliases.get(&first).filter(|v| simple(v)) else { break };
            if seen.contains(&first) {
                break; // a cycle
            }
            let self_named = value.split_whitespace().next() == Some(first.as_str()); // alias ls='ls -G'
            head = format!("{}{}", value.trim(), &head[first.len()..]);
            seen.push(first);
            if self_named {
                break;
            }
        }
        if head == original {
            head.clear();
        }
        if head.is_empty() {
            return None;
        }
        let delta = head.len() as isize - first_end as isize;
        Some(Expanded { buffer: format!("{head}{}", &buffer[first_end..]), cursor: (cursor as isize + delta) as usize, delta })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(pairs: &[(&str, &str)]) -> ShellContext {
        ShellContext { aliases: pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(), ..Default::default() }
    }

    #[test]
    fn an_alias_is_replaced_by_its_expansion_and_offsets_shift() {
        let e = ctx(&[("gco", "git checkout")]).expand("gco ma", 6).unwrap();
        assert_eq!((e.buffer.as_str(), e.cursor, e.delta), ("git checkout ma", 15, 9));
    }

    #[test]
    fn aliases_chain_but_never_loop() {
        let c = ctx(&[("g", "git"), ("gs", "g status"), ("ls", "ls -G"), ("a", "b"), ("b", "a")]);
        assert_eq!(c.expand("gs --", 5).unwrap().buffer, "git status --");
        assert_eq!(c.expand("ls -", 4).unwrap().buffer, "ls -G -", "an alias for itself expands once");
        assert!(c.expand("a x", 3).is_none(), "a cycle is left alone");
    }

    #[test]
    fn nothing_to_expand_while_the_command_is_still_being_typed_or_when_it_is_complex() {
        let c = ctx(&[("g", "git"), ("ll", "ls -l | less"), ("e", "echo $HOME")]);
        assert!(c.expand("g", 1).is_none());
        assert!(c.expand("g x", 1).is_none(), "cursor inside the command word");
        assert!(c.expand("ll x", 4).is_none() && c.expand("e x", 3).is_none());
        assert!(c.expand("plain x", 7).is_none());
    }
}
