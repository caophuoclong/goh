//! Fixed read-only Docker, git (changed files, commits), kubectl (pods, namespaces), zoxide and directory-history
//! queries. No shell, spec scripts, or buffer text reaches a subprocess.
use std::collections::HashMap;
use std::io::{ErrorKind, Read};
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use goh_core::spec::{Provider, Suggestion};

/// How long a request waits for a program to answer. A slower one finishes in the background, and its answer is used
/// from the next keystroke on (its last answer meanwhile).
const DEADLINE: Duration = Duration::from_millis(120);
const TTL: Duration = Duration::from_secs(2);
const MAX_BYTES: usize = 512 * 1024;
/// A program asked in the background (Docker on a loaded host, a cluster) may take this long before it is killed.
const BACKGROUND_DEADLINE: Duration = Duration::from_secs(5);
/// A cluster can take seconds to answer: no request waits for it, and what it said last is used for longer.
const CLUSTER_TTL: Duration = Duration::from_secs(15);

/// Keyed by provider and, for the ones that depend on the project, the working directory.
type Cache = HashMap<(Provider, PathBuf), (Instant, Vec<Suggestion>)>;

/// What a program said last.
#[derive(Default)]
struct Answer {
    at: Option<Instant>,
    asking: bool,
    /// It took longer than a request waits: later requests take the last answer at once.
    slow: bool,
    values: Vec<Suggestion>,
}
/// Per provider and what its answer depends on: the working directory for git, the kubectl context for a cluster.
type Asked = HashMap<(Provider, String), Answer>;

pub(crate) struct Providers {
    docker: PathBuf,
    git: PathBuf,
    zoxide: PathBuf,
    kubectl: PathBuf,
    visits: PathBuf,
    home: PathBuf,
    cache: Mutex<Cache>,
    /// Answers of the programs, and the signal that one arrived.
    asked: Arc<(Mutex<Asked>, Condvar)>,
}

impl Default for Providers {
    fn default() -> Self {
        Self {
            docker: PathBuf::from("docker"),
            git: PathBuf::from("git"),
            zoxide: PathBuf::from("zoxide"),
            kubectl: PathBuf::from("kubectl"),
            visits: crate::visits::path(),
            home: std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default(),
            cache: Mutex::new(HashMap::new()),
            asked: Arc::default(),
        }
    }
}

impl Providers {
    pub fn values(&self, provider: Provider, cwd: &Path) -> Vec<Suggestion> {
        if matches!(provider, Provider::KubePods | Provider::KubeNamespaces) {
            // Nothing until the first answer, and nothing once it fails (another context's pods would be wrong).
            let context = crate::sources::values(Provider::KubeContexts, &self.home)
                .into_iter()
                .find(|s| s.description.as_deref() == Some("current context"))
                .map(|s| s.name)
                .unwrap_or_default();
            return self.ask(provider, context, &self.kubectl, Path::new("/"), CLUSTER_TTL, Duration::ZERO);
        }
        let status = matches!(provider, Provider::GitChangedFiles | Provider::GitStagedFiles | Provider::GitCommits);
        let project = status || crate::project::reads_project(provider) || crate::packages::reads_cwd(provider);
        let local = matches!(provider, Provider::SshHosts | Provider::SshRemotes | Provider::KubeContexts | Provider::AwsProfiles);
        if !local && !crate::packages::reads(provider) && !(project && !status) {
            // Only git works where you are; the other programs are asked from `/`.
            let (bin, key, dir) = match provider {
                Provider::FrecentDirs => (&self.zoxide, String::new(), Path::new("/")),
                _ if status => (&self.git, cwd.to_string_lossy().into_owned(), cwd),
                _ => (&self.docker, String::new(), Path::new("/")),
            };
            let values = self.ask(provider, key, bin, dir, TTL, DEADLINE);
            if provider == Provider::FrecentDirs && values.is_empty() {
                // zoxide missing, failing, slow or empty: use goh's own history.
                return crate::visits::ranked(&self.visits, crate::visits::now())
                    .into_iter()
                    .map(|name| Suggestion { name, description: None, insert_value: None })
                    .collect();
            }
            return values;
        }
        let key = (provider, if project { cwd.to_path_buf() } else { PathBuf::new() });
        // Never queue input behind another refresh.
        let Ok(mut cache) = self.cache.try_lock() else {
            return vec![];
        };
        if let Some((_, values)) = cache.get(&key).filter(|(at, _)| at.elapsed() < TTL) {
            return values.clone();
        }
        let values = if crate::packages::reads(provider) {
            crate::packages::values(provider, &self.home, cwd)
        } else if project {
            crate::project::values(provider, cwd)
        } else {
            crate::sources::values(provider, &self.home)
        };
        cache.insert(key, (Instant::now(), values.clone()));
        values
    }

    /// What `bin` answers for `provider`: asked again in the background once its last answer is `ttl` old, waiting
    /// up to `wait` for it unless it was slower than that last time; what it said last otherwise (nothing before its
    /// first answer).
    fn ask(&self, provider: Provider, key: String, bin: &Path, dir: &Path, ttl: Duration, wait: Duration) -> Vec<Suggestion> {
        let (lock, arrived) = &*self.asked;
        let Ok(mut asked) = lock.lock() else { return vec![] };
        let key = (provider, key);
        let answer = asked.entry(key.clone()).or_default();
        let wait = if answer.slow { Duration::ZERO } else { wait };
        if !answer.asking && answer.at.is_none_or(|at| at.elapsed() >= ttl) {
            answer.asking = true;
            let (bin, dir, store, key) = (bin.to_path_buf(), dir.to_path_buf(), Arc::clone(&self.asked), key.clone());
            std::thread::spawn(move || {
                let start = Instant::now();
                let values = query(&bin, provider, BACKGROUND_DEADLINE, &dir).map(|text| parse(provider, &text)).unwrap_or_default();
                let (lock, arrived) = &*store;
                if let Ok(mut asked) = lock.lock() {
                    *asked.entry(key).or_default() = Answer { at: Some(Instant::now()), asking: false, slow: start.elapsed() > wait, values };
                }
                arrived.notify_all();
            });
        }
        if !wait.is_zero() {
            let Ok((waited, _)) = arrived.wait_timeout_while(asked, wait, |a| a.get(&key).is_some_and(|x| x.asking)) else { return vec![] };
            asked = waited;
        }
        asked.get(&key).map(|a| a.values.clone()).unwrap_or_default()
    }
}

/// Git without anything the repository could make it run: no filesystem monitor hook, no optional locks.
const GIT_SAFE: [&str; 3] = ["--no-optional-locks", "-c", "core.fsmonitor=false"];

fn query(docker: &Path, provider: Provider, timeout: Duration, dir: &Path) -> Option<String> {
    let git_changed = [GIT_SAFE.as_slice(), &["ls-files", "-m", "-o", "--exclude-standard", "-z"]].concat();
    let git_staged = [GIT_SAFE.as_slice(), &["diff", "--cached", "--name-only", "--relative", "-z"]].concat();
    let git_commits = [GIT_SAFE.as_slice(), &["log", "-n", "100", "--no-color", "--format=%h%x1f%s"]].concat();
    let args: &[&str] = match provider {
        Provider::GitChangedFiles => &git_changed,
        Provider::GitStagedFiles => &git_staged,
        Provider::GitCommits => &git_commits,
        Provider::KubePods => &["get", "pods", "-o", "name", "--request-timeout=4s"],
        Provider::KubeNamespaces => &["get", "namespaces", "-o", "name", "--request-timeout=4s"],
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
    if provider == Provider::GitCommits {
        // `abc1234<US>subject`, newest first.
        return text
            .lines()
            .take(100)
            .filter_map(|l| l.split_once('\x1f'))
            .filter(|(hash, _)| (4..=64).contains(&hash.len()) && hash.bytes().all(|b| b.is_ascii_hexdigit()))
            .map(|(hash, subject)| Suggestion {
                name: hash.to_owned(),
                description: Some(subject.chars().filter(|c| !c.is_control()).take(80).collect()),
                insert_value: None,
            })
            .collect();
    }
    if let Some((prefix, what)) = match provider {
        Provider::KubePods => Some(("pod/", "pod")),
        Provider::KubeNamespaces => Some(("namespace/", "namespace")),
        _ => None,
    } {
        // `pod/name`, one per line; names are DNS labels.
        let mut seen = std::collections::HashSet::new();
        return text
            .lines()
            .take(5000)
            .filter_map(|l| l.trim().strip_prefix(prefix))
            .filter(|n| !n.is_empty() && n.len() <= 253 && n.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'.') && !n.starts_with('-') && seen.insert(n.to_string()))
            .map(|n| Suggestion { name: n.to_owned(), description: Some(what.to_owned()), insert_value: None })
            .collect();
    }
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
        // Linux refuses to exec a file while any process holds a write descriptor to it, and a sibling test's
        // fork can briefly hold a copy of the one just closed above. Wait it out here, not in `query`.
        for _ in 0..200 {
            match Command::new(&p).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn() {
                Err(e) if e.raw_os_error() == Some(libc::ETXTBSY) => std::thread::sleep(Duration::from_millis(10)),
                Ok(mut child) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    break;
                }
                Err(_) => break,
            }
        }
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
            kubectl: PathBuf::from("kubectl"),
            visits: dir.path().join("none"),
            home: dir.path().to_path_buf(),
            cache: Mutex::default(),
            asked: Arc::new((
                Mutex::new(HashMap::from([((Provider::DockerContainers, String::new()), Answer { at: Some(Instant::now()), asking: false, slow: false, values })])),
                Condvar::new(),
            )),
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
            ..Providers::default()
        };
        let names: Vec<_> = providers.values(Provider::FrecentDirs, Path::new("/")).into_iter().map(|v| v.name).collect();
        assert_eq!(names, [dir.path().to_str().unwrap()]);
    }

    #[test]
    fn pip_packages_are_cached_per_project() {
        let root = tempfile::tempdir().unwrap();
        for (project, package) in [("one", "alpha-1.0.dist-info"), ("two", "beta-2.0.dist-info")] {
            std::fs::create_dir_all(root.path().join(project).join(".venv/lib/python3.12/site-packages").join(package)).unwrap();
        }
        let providers = Providers { home: root.path().to_path_buf(), ..Providers::default() };
        let names = |project: &str| providers.values(Provider::PipPackages, &root.path().join(project)).into_iter().map(|s| s.name).collect::<Vec<_>>();
        assert_eq!((names("one"), names("two")), (vec!["alpha".to_string()], vec!["beta".to_string()]));
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
    fn commits_pods_and_namespaces_are_validated() {
        let commits = parse(Provider::GitCommits, "abc1234\x1ffix: the thing\nnot-hex\x1fbad\nabc\x1ftoo short\ndef5678\x1fbell\x07 here\n");
        assert_eq!(commits.iter().map(|c| (c.name.as_str(), c.description.as_deref().unwrap())).collect::<Vec<_>>(), [("abc1234", "fix: the thing"), ("def5678", "bell here")]);
        let pods = parse(Provider::KubePods, "pod/web-1\npod/Bad_Name\nservice/x\npod/web-1\npod/api.v2\n");
        assert_eq!(pods.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), ["web-1", "api.v2"]);
        assert_eq!(parse(Provider::KubeNamespaces, "namespace/default\nnamespace/kube-system\n")[1].name, "kube-system");
    }

    #[test]
    fn a_cluster_is_asked_in_the_background_and_its_last_answer_used() {
        let dir = tempfile::tempdir().unwrap();
        let kubectl = script(dir.path(), "[ \"$1 $2\" = 'get pods' ] || exit 1\nsleep 0.3\necho pod/web-1");
        let providers = Providers { kubectl, home: dir.path().to_path_buf(), ..Providers::default() };
        let at = Instant::now();
        assert!(providers.values(Provider::KubePods, Path::new("/")).is_empty(), "the first request does not wait for the cluster");
        assert!(at.elapsed() < Duration::from_millis(200));
        let deadline = Instant::now() + Duration::from_secs(5);
        while providers.values(Provider::KubePods, Path::new("/")).is_empty() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(providers.values(Provider::KubePods, Path::new("/"))[0].name, "web-1");
    }

    #[test]
    fn a_slow_docker_answers_from_the_next_request_on() {
        let dir = tempfile::tempdir().unwrap();
        let docker = script(dir.path(), "sleep 0.3\necho '{\"Names\":\"web-1\",\"Image\":\"nginx\"}'");
        let providers = Providers { docker, home: dir.path().to_path_buf(), ..Providers::default() };
        let at = Instant::now();
        assert!(providers.values(Provider::DockerContainers, Path::new("/")).is_empty(), "past the deadline: nothing yet");
        assert!(at.elapsed() < Duration::from_millis(250), "a slow Docker cannot block typing");
        let deadline = Instant::now() + Duration::from_secs(5);
        while providers.values(Provider::DockerContainers, Path::new("/")).is_empty() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(providers.values(Provider::DockerContainers, Path::new("/"))[0].name, "web-1");
        std::thread::sleep(TTL);
        let at = Instant::now();
        assert_eq!(providers.values(Provider::DockerContainers, Path::new("/"))[0].name, "web-1", "the last answer while it is asked again");
        assert!(at.elapsed() < Duration::from_millis(50), "a Docker known to be slow is not waited for again");
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
