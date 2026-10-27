//! Suggestions read from the user's own files: where they have connected (ssh config, known hosts, shell
//! history), kubectl contexts, AWS profiles. Plain file reads, bounded in size, nothing executed.

use std::collections::HashSet;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use goh_core::spec::{Provider, Suggestion};

const MAX_FILE: u64 = 512 * 1024;
const MAX_NAMES: usize = 2000;

fn suggestion(name: String, description: &str, insert_value: Option<String>) -> Suggestion {
    Suggestion { name, description: Some(description.to_string()), insert_value }
}

/// The last `MAX_FILE` bytes of a text file (history files grow without bound; the recent end is what counts).
fn read_tail(path: &Path) -> Option<String> {
    let mut f = std::fs::File::open(path).ok()?;
    let len = f.metadata().ok()?.len();
    f.seek(SeekFrom::Start(len.saturating_sub(MAX_FILE))).ok()?;
    let mut bytes = Vec::new();
    f.take(MAX_FILE).read_to_end(&mut bytes).ok()?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

pub fn values(provider: Provider, home: &Path) -> Vec<Suggestion> {
    match provider {
        Provider::SshHosts => ssh_hosts(home).into_iter().map(|(n, d)| suggestion(n, d, None)).collect(),
        // `scp`/`rsync` want `host:` and no space after it.
        Provider::SshRemotes => ssh_hosts(home).into_iter().map(|(n, d)| suggestion(n.clone(), d, Some(format!("{n}:")))).collect(),
        Provider::KubeContexts => kube_contexts(home),
        Provider::AwsProfiles => aws_profiles(home),
        _ => vec![],
    }
}

// ---- ssh --------------------------------------------------------------------------------------

fn plain_host(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 255
        && !s.starts_with('-')
        && s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '@' | '%' | '+'))
}

/// `Host` aliases of an ssh config (patterns with `*?!` are not hosts you can connect to).
pub fn ssh_config_hosts(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in text.lines() {
        let mut words = line.split_whitespace();
        if words.next().is_some_and(|k| k.eq_ignore_ascii_case("host")) {
            out.extend(words.filter(|w| plain_host(w)).map(str::to_string));
        }
    }
    out
}

/// Hosts named in a `known_hosts` file; hashed entries cannot be read and are skipped.
pub fn known_hosts(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in text.lines() {
        let mut fields = line.split_whitespace();
        let Some(mut first) = fields.next() else { continue };
        if first.starts_with('@') {
            let Some(next) = fields.next() else { continue };
            first = next;
        }
        if first.starts_with('#') || first.starts_with('|') {
            continue;
        }
        for h in first.split(',') {
            let h = h.strip_prefix('[').map_or(h, |r| r.split(']').next().unwrap_or(r));
            if plain_host(h) {
                out.push(h.to_string());
            }
        }
    }
    out
}

const VALUE_OPTIONS: &str = "bcDEeFIiJLlmOopQRSWwB";

fn history_command(line: &str) -> &str {
    if let Some(rest) = line.strip_prefix(": ") {
        return rest.split_once(';').map_or(rest, |(_, c)| c); // zsh extended history
    }
    line.trim_start().strip_prefix("- cmd: ").unwrap_or(line) // fish history, else plain
}

/// Where the commands in a shell history connected to, newest first: `[user@]host` after `ssh`, `mosh`,
/// `sftp`, `ssh-copy-id`, and the `host:` part of `scp`/`rsync` arguments.
pub fn history_hosts(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in text.lines().rev() {
        let words: Vec<&str> = history_command(line).split_whitespace().collect();
        match words.first().copied() {
            Some("ssh" | "mosh" | "sftp" | "ssh-copy-id") => {
                let mut i = 1;
                while i < words.len() {
                    let w = words[i];
                    if let Some(opt) = w.strip_prefix('-') {
                        // `-p 22` takes the next word; `-p22` and flags without values do not.
                        i += if opt.len() == 1 && VALUE_OPTIONS.contains(opt) { 2 } else { 1 };
                        continue;
                    }
                    let dest = w.strip_prefix("ssh://").unwrap_or(w);
                    if plain_host(dest) {
                        out.push(dest.to_string());
                    }
                    break;
                }
            }
            Some("scp" | "rsync") => {
                for w in &words[1..] {
                    if let Some((host, rest)) = w.split_once(':') {
                        if !host.contains('/') && !rest.starts_with("//") && plain_host(host) {
                            out.push(host.to_string());
                        }
                    }
                }
            }
            _ => {}
        }
    }
    out
}

/// Where you have been, then what you configured, then what you have seen. Deduplicated.
fn ssh_hosts(home: &Path) -> Vec<(String, &'static str)> {
    let ssh = home.join(".ssh");
    let mut sources: Vec<(Vec<String>, &'static str)> = Vec::new();
    let history: Vec<String> = [".zsh_history", ".bash_history", ".local/share/fish/fish_history"]
        .iter()
        .filter_map(|f| read_tail(&home.join(f)))
        .flat_map(|t| history_hosts(&t))
        .collect();
    sources.push((history, "recent"));
    sources.push((config_files(&ssh).iter().filter_map(|p| std::fs::read_to_string(p).ok()).flat_map(|t| ssh_config_hosts(&t)).collect(), "ssh config"));
    sources.push((["known_hosts", "known_hosts2"].iter().filter_map(|f| read_tail(&ssh.join(f))).flat_map(|t| known_hosts(&t)).collect(), "known host"));
    let mut seen = HashSet::new();
    sources.into_iter().flat_map(|(names, d)| names.into_iter().map(move |n| (n, d))).filter(|(n, _)| seen.insert(n.clone())).take(MAX_NAMES).collect()
}

/// `~/.ssh/config` and what it `Include`s (one level, `*` allowed in the file name).
fn config_files(ssh: &Path) -> Vec<PathBuf> {
    let main = ssh.join("config");
    let mut files = vec![main.clone()];
    let Ok(text) = std::fs::read_to_string(&main) else { return files };
    for line in text.lines() {
        let mut words = line.split_whitespace();
        if !words.next().is_some_and(|k| k.eq_ignore_ascii_case("include")) {
            continue;
        }
        for pattern in words {
            let path = ssh.join(pattern.strip_prefix("~/.ssh/").unwrap_or(pattern));
            let (Some(dir), Some(name)) = (path.parent(), path.file_name().and_then(|n| n.to_str())) else { continue };
            let Some((pre, post)) = name.split_once('*') else {
                files.push(path);
                continue;
            };
            let mut matched: Vec<PathBuf> = std::fs::read_dir(dir)
                .into_iter()
                .flatten()
                .flatten()
                .filter(|e| e.file_name().to_str().is_some_and(|n| n.starts_with(pre) && n.ends_with(post) && n.len() >= pre.len() + post.len()))
                .map(|e| e.path())
                .filter(|p| p.is_file())
                .collect();
            matched.sort();
            files.extend(matched);
        }
    }
    files
}

// ---- kubectl, aws -----------------------------------------------------------------------------

/// Context names in a kubeconfig, the current one marked.
pub fn kube_context_names(text: &str) -> (Vec<String>, Option<String>) {
    let (mut names, mut current, mut in_contexts) = (Vec::new(), None, false);
    let unquote = |v: &str| v.trim().trim_matches(['"', '\'']).to_string();
    for line in text.lines() {
        if let Some(v) = line.strip_prefix("current-context:") {
            current = Some(unquote(v)).filter(|c| !c.is_empty());
        }
        if !line.starts_with([' ', '-', '\t']) && !line.trim().is_empty() {
            in_contexts = line.starts_with("contexts:"); // a new top-level key ends the block
            continue;
        }
        if in_contexts {
            // Inside `contexts:` the only `name:` keys are the contexts' own (cluster and user use other keys).
            let t = line.trim_start().trim_start_matches("- ");
            if let Some(v) = t.strip_prefix("name:") {
                names.push(unquote(v));
            }
        }
    }
    (names, current)
}

fn kube_contexts(home: &Path) -> Vec<Suggestion> {
    let file = std::env::var_os("KUBECONFIG")
        .and_then(|v| std::env::split_paths(&v).next())
        .unwrap_or_else(|| home.join(".kube/config"));
    let Some(text) = read_tail(&file) else { return vec![] };
    let (names, current) = kube_context_names(&text);
    names.into_iter().filter(|n| plain_host(n) || n.chars().all(|c| !c.is_control() && !c.is_whitespace())).map(|n| {
        let d = if current.as_deref() == Some(n.as_str()) { "current context" } else { "kube context" };
        suggestion(n, d, None)
    }).collect()
}

/// `[profile name]` and `[default]` of an AWS config, `[name]` of a credentials file.
pub fn aws_profile_names(text: &str, credentials: bool) -> Vec<String> {
    text.lines()
        .filter_map(|l| l.trim().strip_prefix('[')?.strip_suffix(']'))
        .filter_map(|h| if credentials { Some(h) } else { h.strip_prefix("profile ").or((h == "default").then_some(h)) })
        .map(|n| n.trim().to_string())
        .filter(|n| !n.is_empty() && n.chars().all(|c| !c.is_control() && !c.is_whitespace()))
        .collect()
}

fn aws_profiles(home: &Path) -> Vec<Suggestion> {
    let config = std::env::var_os("AWS_CONFIG_FILE").map_or_else(|| home.join(".aws/config"), PathBuf::from);
    let credentials = std::env::var_os("AWS_SHARED_CREDENTIALS_FILE").map_or_else(|| home.join(".aws/credentials"), PathBuf::from);
    let mut seen = HashSet::new();
    let from = |p: &Path, creds| read_tail(p).map(|t| aws_profile_names(&t, creds)).unwrap_or_default();
    [from(&config, false), from(&credentials, true)]
        .concat()
        .into_iter()
        .filter(|n| seen.insert(n.clone()))
        .map(|n| suggestion(n, "AWS profile", None))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ssh_config_aliases_skip_patterns() {
        assert_eq!(ssh_config_hosts("Host prod staging\n  HostName x\nhost *.internal db?\nHost !bad ok\n"), ["prod", "staging", "ok"]);
    }

    #[test]
    fn known_hosts_skips_hashes_and_unwraps_ports() {
        let t = "github.com,140.82.1.1 ssh-ed25519 AAA\n|1|abc= ssh-rsa AAA\n[box.example.com]:2222 ssh-rsa AAA\n@cert-authority corp.example ssh-rsa AAA\n# c\n";
        assert_eq!(known_hosts(t), ["github.com", "140.82.1.1", "box.example.com", "corp.example"]);
    }

    #[test]
    fn history_gives_destinations_newest_first_across_shell_formats() {
        let t = ": 1:0;ssh -p 2222 -i ~/.ssh/k admin@old.example\nls\n- cmd: ssh deploy@web1\nssh -o StrictHostKeyChecking=no 10.0.0.5 uptime\nscp ./a.txt me@files.example:/tmp/\nrsync -av src/ build.example:dst\nssh -vvv ssh://u@jump.example\ncat http://x/y\n";
        assert_eq!(history_hosts(t), ["u@jump.example", "build.example", "me@files.example", "10.0.0.5", "deploy@web1", "admin@old.example"]);
    }

    #[test]
    fn hosts_merge_recent_then_config_then_known_without_duplicates() {
        let home = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(home.path().join(".ssh/conf.d")).unwrap();
        std::fs::write(home.path().join(".zsh_history"), ": 1:0;ssh alice@prod\n").unwrap();
        std::fs::write(home.path().join(".ssh/config"), "Include conf.d/*.conf\nHost prod\nHost staging\n").unwrap();
        std::fs::write(home.path().join(".ssh/conf.d/extra.conf"), "Host extra\n").unwrap();
        std::fs::write(home.path().join(".ssh/known_hosts"), "staging ssh-rsa A\ngit.example ssh-rsa A\n").unwrap();
        let hosts = ssh_hosts(home.path());
        assert_eq!(hosts.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>(), ["alice@prod", "prod", "staging", "extra", "git.example"]);
        assert_eq!(hosts[0].1, "recent");
        let remotes = values(Provider::SshRemotes, home.path());
        assert_eq!(remotes[1].insert_value.as_deref(), Some("prod:"));
    }

    #[test]
    fn kube_contexts_are_the_names_inside_the_contexts_block_only() {
        let t = "apiVersion: v1\nclusters:\n- cluster:\n    server: https://x\n  name: kind-c\ncontexts:\n- context:\n    cluster: kind-c\n    user: u\n  name: dev\n- name: \"prod\"\n  context:\n    cluster: p\ncurrent-context: prod\nusers:\n- name: u\n";
        assert_eq!(kube_context_names(t), (vec!["dev".to_string(), "prod".to_string()], Some("prod".to_string())));
    }

    #[test]
    fn aws_profiles_come_from_config_and_credentials() {
        assert_eq!(aws_profile_names("[default]\nregion=x\n[profile work]\n[sso-session s]\n", false), ["default", "work"]);
        assert_eq!(aws_profile_names("[default]\n[ci]\n", true), ["default", "ci"]);
    }
}
