//! Fixed read-only Docker, git (changed files), zoxide and directory-history queries. No shell, spec scripts, or buffer text reaches a subprocess.
use std::collections::HashMap;
use std::io::{ErrorKind, Read};
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use goh_core::spec::{Provider, Suggestion};

const DEADLINE: Duration = Duration::from_millis(120);
const TTL: Duration = Duration::from_secs(2);
const MAX_BYTES: usize = 512 * 1024;

/// Keyed by provider and, for the ones that depend on the project, the working directory.
type Cache = HashMap<(Provider, PathBuf), (Instant, Vec<Suggestion>)>;

pub(crate) struct Providers {
    docker: PathBuf,
    git: PathBuf,
    zoxide: PathBuf,
    visits: PathBuf,
    home: PathBuf,
    cache: Mutex<Cache>,
}

impl Default for Providers {
    fn default() -> Self {
        Self {
            docker: PathBuf::from("docker"),
            git: PathBuf::from("git"),
            zoxide: PathBuf::from("zoxide"),
            visits: crate::visits::path(),
            home: std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default(),
            cache: Mutex::new(HashMap::new()),
        }
    }
}

impl Providers {
    pub fn values(&self, provider: Provider, cwd: &Path) -> Vec<Suggestion> {
        let status = matches!(provider, Provider::GitChangedFiles | Provider::GitStagedFiles);
        let project = status || crate::project::reads_project(provider);
        let key = (provider, if project { cwd.to_path_buf() } else { PathBuf::new() });
        // Never queue input behind another refresh; another session may be using a slow Docker host.
        let Ok(mut cache) = self.cache.try_lock() else {
            return vec![];
        };
        if let Some((_, values)) = cache.get(&key).filter(|(at, _)| at.elapsed() < TTL) {
            return values.clone();
        }
        let local = matches!(provider, Provider::SshHosts | Provider::SshRemotes | Provider::KubeContexts | Provider::AwsProfiles);
        if local || (project && !status) {
            let values = if project { crate::project::values(provider, cwd) } else { crate::sources::values(provider, &self.home) };
            cache.insert(key, (Instant::now(), values.clone()));
            return values;
        }
        let bin = match provider {
            Provider::FrecentDirs => &self.zoxide,
            _ if status => &self.git,
            _ => &self.docker,
        };
        // Only git works where you are; the other programs are asked from `/`.
        let dir = if status { cwd } else { Path::new("/") };
        let mut values = query(bin, provider, DEADLINE, dir)
            .map(|text| parse(provider, &text))
            .unwrap_or_default();
        if provider == Provider::FrecentDirs && values.is_empty() {
            // zoxide missing, failing or empty: use goh's own history.
            values = crate::visits::ranked(&self.visits, crate::visits::now())
                .into_iter()
                .map(|name| Suggestion { name, description: None, insert_value: None })
                .collect();
        }
        cache.insert(key, (Instant::now(), values.clone()));
        values
    }
}

/// Git without anything the repository could make it run: no filesystem monitor hook, no optional locks.
const GIT_SAFE: [&str; 3] = ["--no-optional-locks", "-c", "core.fsmonitor=false"];

fn query(docker: &Path, provider: Provider, timeout: Duration, dir: &Path) -> Option<String> {
    let git_changed = [GIT_SAFE.as_slice(), &["ls-files", "-m", "-o", "--exclude-standard", "-z"]].concat();
    let git_staged = [GIT_SAFE.as_slice(), &["diff", "--cached", "--name-only", "--relative", "-z"]].concat();
    let args: &[&str] = match provider {
        Provider::GitChangedFiles => &git_changed,
        Provider::GitStagedFiles => &git_staged,
        Provider::DockerContainers => &["ps", "--format", "{{json .}}"],
        Provider::DockerAllContainers => &["ps", "--all", "--format", "{{json .}}"],
        Provider::DockerImages => &["image", "ls", "--format", "{{json .}}"],
        Provider::FrecentDirs => &["query", "--list"],
        // Read from files by `sources`, never queried through a program.
        Provider::SshHosts | Provider::SshRemotes | Provider::KubeContexts | Provider::AwsProfiles => return None,
        _ => return None,
    };
    let mut child = Command::new(docker)
        .args(args)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .current_dir(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let result = (|| {
        let mut stdout = child.stdout.take()?;
        let fd = stdout.as_raw_fd();
        // SAFETY: our live pipe FD; preserve flags and add nonblocking reads to enforce the deadline.
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
        if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
            return None;
        }
        let start = Instant::now();
        let mut bytes = Vec::new();
        loop {
            let mut chunk = [0; 4096];
            loop {
                match stdout.read(&mut chunk) {
                    Ok(0) => break,
                    Ok(n) => {
                        if bytes.len() + n > MAX_BYTES || start.elapsed() >= timeout {
                            return None;
                        }
                        bytes.extend_from_slice(&chunk[..n]);
                    }
                    Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                    Err(e) if e.kind() == ErrorKind::Interrupted => continue,
                    Err(_) => return None,
                }
            }
            if let Some(status) = child.try_wait().ok()? {
                if !status.success() {
                    return None;
                }
                // Drain bytes written between the last read and process exit; never wait on a
                // descendant holding the pipe. A complete JSON row is required by the parser.
                let mut tail = Vec::new();
                stdout
                    .take((MAX_BYTES - bytes.len() + 1) as u64)
                    .read_to_end(&mut tail)
                    .ok()?;
                bytes.extend(tail);
                if bytes.len() > MAX_BYTES {
                    return None;
                }
                return String::from_utf8(bytes).ok();
            }
            if start.elapsed() >= timeout {
                return None;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    })();
    // Reap on every path, including malformed/oversized output and deadlines.
    let _ = child.kill();
    let _ = child.wait();
    result
}

fn parse(provider: Provider, text: &str) -> Vec<Suggestion> {
    if matches!(provider, Provider::GitChangedFiles | Provider::GitStagedFiles) {
        // Paths relative to the working directory, NUL-separated (no quoting to undo).
        let note = if provider == Provider::GitStagedFiles { "staged" } else { "changed or new" };
        let mut seen = std::collections::HashSet::new();
        return text
            .split('\0')
            .take(5000)
            .filter(|p| !p.is_empty() && p.len() <= 1024 && !p.starts_with('-') && !p.chars().any(char::is_control) && seen.insert(*p))
            .map(|p| Suggestion { name: p.to_owned(), description: Some(note.to_owned()), insert_value: None })
            .collect();
    }
    if provider == Provider::FrecentDirs {
        let mut seen = std::collections::HashSet::new();
        // One absolute path per line, most frecent first. The engine escapes them for the shell.
        return text
            .lines()
            .take(5000)
            .filter(|l| l.starts_with('/') && l.len() <= 1024 && !l.chars().any(char::is_control) && seen.insert(*l))
            .map(|l| Suggestion { name: l.to_owned(), description: None, insert_value: None })
            .collect();
    }
    let mut seen = std::collections::HashSet::new();
    text.lines()
        .take(5000)
        .filter_map(|line| {
            let row: serde_json::Value = serde_json::from_str(line).ok()?;
            let name = if provider == Provider::DockerImages {
                let repository = row["Repository"].as_str()?;
                let tag = row["Tag"].as_str()?;
                if repository == "<none>" || tag == "<none>" {
                    return None;
                }
                format!("{repository}:{tag}")
            } else {
                row["Names"].as_str()?.to_owned()
            };
            if !name
                .bytes()
                .next()
                .is_some_and(|b| b.is_ascii_alphanumeric())
                || name.len() > 256
                || !name
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"_./:@-".contains(&b))
                || !seen.insert(name.clone())
            {
                return None;
            }
            let description = row[if provider == Provider::DockerImages {
                "ID"
            } else {
                "Image"
            }]
            .as_str()
            .filter(|s| s.len() <= 256 && !s.chars().any(char::is_control))
            .map(str::to_owned);
            Some(Suggestion {
                name,
                description,
                insert_value: None,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn script(dir: &Path, body: &str) -> PathBuf {
        let p = dir.join("docker");
        std::fs::write(&p, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o700)).unwrap();
        p
    }

    #[test]
    fn fixed_queries_parse_cache_and_reject_bad_output() {
        let dir = tempfile::tempdir().unwrap();
        let docker = script(dir.path(), "[ \"$*\" = 'ps --format {{json .}}' ] || exit 1\nprintf '%s\\n' '{\"Names\":\"web-1\",\"Image\":\"nginx\"}' '{\"Names\":\"bad;touch /tmp/no\"}' '{\"Names\":\"--help\"}' '{\"Names\":\"web-1\"}'");
        // Give process-launch assertions a generous test deadline. Production uses DEADLINE;
        // the next test verifies that slow providers are killed within that smaller budget.
        let text = query(&docker, Provider::DockerContainers, Duration::from_secs(1), Path::new("/")).unwrap();
        let values = parse(Provider::DockerContainers, &text);
        assert_eq!(
            values.iter().map(|v| v.name.as_str()).collect::<Vec<_>>(),
            ["web-1"]
        );
        let providers = Providers {
            docker: docker.clone(),
            git: PathBuf::from("git"),
            zoxide: PathBuf::from("zoxide"),
            visits: dir.path().join("none"),
            home: dir.path().to_path_buf(),
            cache: Mutex::new(HashMap::from([(
                (Provider::DockerContainers, PathBuf::new()),
                (Instant::now(), values),
            )])),
        };
        std::fs::remove_file(docker).unwrap();
        assert_eq!(
            providers.values(Provider::DockerContainers, Path::new("/")).len(),
            1,
            "a second query uses the cache"
        );
        assert!(
            providers.values(Provider::DockerImages, Path::new("/")).is_empty(),
            "missing Docker fails open"
        );
        assert_eq!(parse(Provider::FrecentDirs, "/a b\nrelative\n/a b\n/c\x07d\n/e").iter().map(|v| v.name.as_str()).collect::<Vec<_>>(), ["/a b", "/e"]);
        assert_eq!(parse(Provider::DockerImages, "{\"Repository\":\"nginx\",\"Tag\":\"alpine\",\"ID\":\"123\"}\n{\"Repository\":\"<none>\",\"Tag\":\"<none>\"}\nmalformed")[0].name, "nginx:alpine");
    }

    #[test]
    fn frecent_dirs_fall_back_to_own_history_without_zoxide() {
        let dir = tempfile::tempdir().unwrap();
        let visits = dir.path().join("dirs");
        crate::visits::record(&visits, dir.path().to_str().unwrap(), crate::visits::now()).unwrap();
        let providers = Providers {
            docker: PathBuf::from("docker"),
            git: PathBuf::from("git"),
            zoxide: dir.path().join("no-zoxide"),
            visits,
            home: dir.path().to_path_buf(),
            cache: Mutex::new(HashMap::new()),
        };
        let names: Vec<_> = providers.values(Provider::FrecentDirs, Path::new("/")).into_iter().map(|v| v.name).collect();
        assert_eq!(names, [dir.path().to_str().unwrap()]);
    }

    #[test]
    fn changed_and_staged_files_come_from_git_relative_to_the_working_directory() {
        let git = |dir: &Path, args: &[&str]| Command::new("git").args(args).current_dir(dir).env("GIT_CONFIG_GLOBAL", "/dev/null").status().is_ok_and(|s| s.success());
        let repo = tempfile::tempdir().unwrap();
        let d = repo.path();
        if !git(d, &["init", "-q"]) {
            return; // no git here: the provider fails open, which the next assertions do not cover
        }
        std::fs::create_dir_all(d.join("src")).unwrap();
        for f in ["src/a.rs", "src/b.rs", "kept.txt"] {
            std::fs::write(d.join(f), "1").unwrap();
        }
        assert!(git(d, &["add", "."]) && git(d, &["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-qm", "x"]));
        std::fs::write(d.join("src/a.rs"), "2").unwrap(); // modified
        std::fs::write(d.join("src/new file.rs"), "1").unwrap(); // untracked, with a space
        std::fs::write(d.join("src/b.rs"), "2").unwrap();
        assert!(git(d, &["add", "src/b.rs"])); // staged
        let providers = Providers::default();
        let names = |p: Provider, dir: &Path| {
            let mut v: Vec<_> = providers.values(p, dir).into_iter().map(|s| s.name).collect();
            v.sort();
            v
        };
        assert_eq!(names(Provider::GitChangedFiles, d), ["src/a.rs", "src/new file.rs"]);
        assert_eq!(names(Provider::GitStagedFiles, d), ["src/b.rs"]);
        // From a subdirectory the paths are relative to it, which is what the command line needs.
        assert_eq!(names(Provider::GitChangedFiles, &d.join("src")), ["a.rs", "new file.rs"]);
        assert!(names(Provider::GitChangedFiles, tempfile::tempdir().unwrap().path()).is_empty(), "outside a repository: nothing");
    }

    #[test]
    fn git_output_is_validated() {
        let rows = parse(Provider::GitChangedFiles, "a.rs\0-rf\0bad\x07name\0a.rs\0b c.rs\0");
        assert_eq!(rows.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(), ["a.rs", "b c.rs"]);
    }

    #[test]
    fn slow_failed_and_oversized_queries_are_bounded() {
        let dir = tempfile::tempdir().unwrap();
        for body in ["while :; do :; done", "exit 1", "while :; do printf 'xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx'; done"] {
            let docker = script(dir.path(), body);
            let at = Instant::now();
            assert!(query(&docker, Provider::DockerContainers, DEADLINE, Path::new("/")).is_none());
            assert!(at.elapsed() < Duration::from_secs(1), "a provider cannot block typing");
        }
    }
}
