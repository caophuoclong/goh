//! `goh spec gen <command>`: a completion spec for a command that has none, read from its own `--help`.
//! Best effort: options (with whether they take a value) and subcommands that appear in the usual layouts
//! (clap, cobra, argparse, getopt-style). Anything it does not understand is left out, never invented.

use serde_json::{json, Value};

/// Drop terminal escape sequences (colours, bold) that some tools print even to a pipe.
pub fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            if chars.next_if_eq(&'[').is_some() {
                while chars.next_if(|c| !c.is_ascii_alphabetic()).is_some() {}
                chars.next();
            }
        } else if c != '\r' {
            out.push(c);
        }
    }
    out
}

fn split_description(rest: &str) -> (&str, &str) {
    // Columns are separated by two or more spaces (or a tab); a single space belongs to the name part.
    let bytes = rest.as_bytes();
    for i in 0..bytes.len() {
        if bytes[i] == b'\t' || (bytes[i] == b' ' && bytes.get(i + 1) == Some(&b' ')) {
            return (&rest[..i], rest[i..].trim());
        }
    }
    (rest, "")
}

/// Value types cobra and argparse print after a flag (`--config string`); a bare word like these means a value.
const TYPE_WORDS: [&str; 17] = ["string", "strings", "int", "ints", "int32", "int64", "uint", "float", "float64", "duration", "file", "path", "dir", "directory", "url", "host", "port"];

fn valid_flag(name: &str) -> bool {
    let body = name.trim_start_matches('-');
    name.starts_with('-') && !body.is_empty() && body.len() <= 40 && body.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '@'))
}

/// One option line: `-v, --verbose`, `--output <FILE>`, `-o, --output=FILE`, `    --[no-]color`.
fn option_line(line: &str) -> Vec<Value> {
    let indent = line.len() - line.trim_start().len();
    let trimmed = line.trim_start();
    if indent == 0 || indent > 12 || !trimmed.starts_with('-') {
        return vec![];
    }
    let (spec, desc) = split_description(trimmed);
    let mut names = Vec::new();
    let mut takes_value = None;
    for part in spec.split([',', ' ']).map(str::trim).filter(|p| !p.is_empty()) {
        let (flag, value) = part.split_once('=').map_or((part, None), |(f, v)| (f, Some(v)));
        if valid_flag(flag) {
            names.push(flag.to_string());
            if let Some(v) = value {
                takes_value = Some(v.trim_matches(['<', '>', '[', ']']).to_string());
            }
        } else if part.starts_with(['<', '[']) || part.chars().next().is_some_and(|c| c.is_ascii_uppercase()) || TYPE_WORDS.contains(&part) {
            takes_value = Some(part.trim_matches(['<', '>', '[', ']', '.']).to_string());
        }
    }
    names
        .into_iter()
        .map(|name| {
            let mut o = json!({ "name": name, "takesValue": takes_value.is_some() });
            if !desc.is_empty() {
                o["description"] = json!(desc.chars().take(300).collect::<String>());
            }
            if let Some(v) = &takes_value {
                o["args"] = json!([{ "name": if v.is_empty() { "value" } else { v.as_str() } }]);
            }
            o
        })
        .collect()
}

fn is_command_heading(line: &str) -> bool {
    let l = line.trim().trim_end_matches(':').to_lowercase();
    !line.starts_with([' ', '\t']) && line.trim_end().ends_with(':') | line.trim().chars().all(|c| !c.is_lowercase()) && (l.ends_with("commands") || l.ends_with("command") || l == "subcommands" || l == "actions")
}

fn valid_subcommand(name: &str) -> bool {
    name.len() <= 40 && name.chars().next().is_some_and(|c| c.is_ascii_alphanumeric()) && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | ':' | '.'))
}

/// A spec from a command's help text.
pub fn from_help(name: &str, help: &str) -> Value {
    let help = strip_ansi(help);
    let lines: Vec<&str> = help.lines().collect();
    let (mut options, mut subs, mut in_commands) = (Vec::<Value>::new(), Vec::<Value>::new(), false);
    let (mut seen_o, mut seen_s) = (std::collections::HashSet::new(), std::collections::HashSet::new());
    for line in &lines {
        if line.trim().is_empty() {
            continue;
        }
        if !line.starts_with([' ', '\t']) {
            in_commands = is_command_heading(line);
            continue;
        }
        if in_commands {
            let trimmed = line.trim_start();
            let (head, desc) = split_description(trimmed);
            // `build, b   Compile` lists aliases; keep the first name and the aliases as subcommands too.
            for (i, n) in head.split(',').map(str::trim).enumerate() {
                if valid_subcommand(n) && seen_s.insert(n.to_string()) && (i == 0 || !desc.is_empty() || head.contains(',')) {
                    let mut s = json!({ "name": n });
                    if !desc.is_empty() {
                        s["description"] = json!(desc.chars().take(300).collect::<String>());
                    }
                    subs.push(s);
                }
            }
            continue;
        }
        for o in option_line(line) {
            if seen_o.insert(o["name"].as_str().unwrap_or_default().to_string()) {
                options.push(o);
            }
        }
    }
    // An option line inside a commands block (e.g. `-h, --help`) is an option, so read those too.
    for line in &lines {
        for o in option_line(line) {
            if seen_o.insert(o["name"].as_str().unwrap_or_default().to_string()) {
                options.push(o);
            }
        }
    }
    let description = lines
        .iter()
        .map(|l| l.trim())
        .find(|l| {
            let lower = l.to_lowercase();
            // Not a usage line, a heading, a flag, or just the program's name and version.
            !l.is_empty() && !lower.starts_with("usage") && !l.ends_with(':') && !l.starts_with('-') && l.len() <= 200 && !(lower.starts_with(&name.to_lowercase()) && l.len() <= 40)
        })
        .unwrap_or("Generated from --help");
    let mut spec = json!({ "schemaVersion": 1, "name": name, "description": description });
    if !options.is_empty() {
        spec["options"] = json!(options);
    }
    if !subs.is_empty() {
        spec["subcommands"] = json!(subs);
    }
    spec
}

/// Run `<name> --help` (or `-h`) and return what it printed. Only a plain command name is accepted, it is not
/// given to a shell, stdin is closed, and it is killed if it takes longer than a few seconds.
pub fn run_help(name: &str) -> Result<String, String> {
    use std::io::Read;
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};
    if name.is_empty() || name.starts_with('-') || !name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '+')) {
        return Err(format!("`{name}` is not a plain command name"));
    }
    let mut last = String::new();
    for flag in ["--help", "-h"] {
        let mut child = Command::new(name)
            .arg(flag)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .env("NO_COLOR", "1")
            .env("TERM", "dumb")
            .env("COLUMNS", "200")
            .spawn()
            .map_err(|e| format!("could not run `{name}`: {e}"))?;
        let readers: Vec<_> = [child.stdout.take().map(|s| Box::new(s) as Box<dyn Read + Send>), child.stderr.take().map(|s| Box::new(s) as Box<dyn Read + Send>)]
            .into_iter()
            .flatten()
            .map(|mut s| std::thread::spawn(move || {
                let mut buf = Vec::new();
                let _ = s.by_ref().take(1 << 20).read_to_end(&mut buf);
                buf
            }))
            .collect();
        let deadline = Instant::now() + Duration::from_secs(5);
        while child.try_wait().map_err(|e| e.to_string())?.is_none() {
            if Instant::now() > deadline {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("`{name} {flag}` did not finish in 5 s"));
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let text: String = readers.into_iter().map(|r| String::from_utf8_lossy(&r.join().unwrap_or_default()).into_owned()).collect::<Vec<_>>().join("\n");
        if text.contains('-') && text.lines().count() > 2 {
            return Ok(text);
        }
        last = text;
    }
    Err(if last.trim().is_empty() { format!("`{name}` printed no help") } else { format!("`{name}` printed no help this can read") })
}

#[cfg(test)]
mod tests {
    use super::*;

    const CLAP: &str = "\x1b[1mrg 14\x1b[0m\nFast search tool\n\nUsage: rg [OPTIONS] PATTERN [PATH]...\n\nOptions:\n  -i, --ignore-case      Case insensitive search\n  -t, --type <TYPE>      Only search files matching TYPE\n      --color=WHEN       When to use color\n  -h, --help             Print help\n";
    const COBRA: &str = "A fast tool.\n\nUsage:\n  tool [command]\n\nAvailable Commands:\n  build       Compile the project\n  completion  Generate shell completion\n  help        Help about any command\n  run, r      Run it\n\nFlags:\n  -c, --config string   config file\n  -v, --verbose\n\nUse \"tool [command] --help\" for more.\n";

    #[test]
    fn clap_style_options_know_which_take_a_value() {
        let spec = from_help("rg", CLAP);
        let opts = spec["options"].as_array().unwrap();
        let by = |n: &str| opts.iter().find(|o| o["name"] == n).unwrap_or_else(|| panic!("{n} in {opts:?}"));
        assert_eq!((by("--ignore-case")["takesValue"].clone(), by("-i")["description"].clone()), (json!(false), json!("Case insensitive search")));
        assert_eq!(by("--type")["takesValue"], true);
        assert_eq!(by("-t")["args"][0]["name"], "TYPE");
        assert_eq!(by("--color")["takesValue"], true, "`--color=WHEN`");
        assert_eq!(spec["description"], "Fast search tool", "colour codes are stripped and Usage is skipped");
    }

    #[test]
    fn cobra_style_subcommands_and_aliases() {
        let spec = from_help("tool", COBRA);
        let subs: Vec<_> = spec["subcommands"].as_array().unwrap().iter().map(|s| s["name"].as_str().unwrap()).collect();
        assert_eq!(subs, ["build", "completion", "help", "run", "r"]);
        assert_eq!(spec["subcommands"][0]["description"], "Compile the project");
        assert!(spec["options"].as_array().unwrap().iter().any(|o| o["name"] == "--config" && o["takesValue"] == true));
    }

    #[test]
    fn output_without_structure_yields_a_valid_empty_spec() {
        let spec = from_help("odd", "something went wrong\nbad flag\n");
        assert!(spec.get("options").is_none() && spec.get("subcommands").is_none());
        goh_core::spec::Command::from_json(&spec.to_string()).unwrap();
    }

    #[test]
    fn generated_specs_load_as_specs() {
        for (n, h) in [("rg", CLAP), ("tool", COBRA)] {
            let c = goh_core::spec::Command::from_json(&from_help(n, h).to_string()).unwrap();
            assert_eq!(c.name, n);
        }
    }
}
