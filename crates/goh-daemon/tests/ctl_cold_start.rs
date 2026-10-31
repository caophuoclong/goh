//! `goh` and a daemon that is missing or has died.

mod common;

use std::fs;
use std::path::Path;
use std::process::{Command, Output};
use std::time::{Duration, Instant};

use common::kill_socket_holder;

fn ctl(socket: &Path, specs: &Path) -> (Output, Duration) {
    let t = Instant::now();
    let out = Command::new(env!("CARGO_BIN_EXE_goh"))
        .args(["menu", "--session", "test-session-1", "--cwd", "/", "--cursor-byte", "5", "--buffer", "git c"])
        .env("GOH_SOCKET", socket)
        .env("GOH_SPEC_DIR", specs)
        .env("GOH_CONFIG", "/nonexistent")
        .output()
        .unwrap();
    (out, t.elapsed())
}

fn labels(out: &Output) -> Vec<String> {
    String::from_utf8_lossy(&out.stdout).lines().filter_map(|l| l.split('\x1f').nth(4)).map(str::to_string).collect()
}

#[test]
fn the_first_request_after_a_cold_start_is_answered() {
    let dir = tempfile::Builder::new().prefix("fc").tempdir_in("/tmp").unwrap();
    fs::create_dir(dir.path().join("specs")).unwrap();
    fs::write(dir.path().join("specs/git.json"), common::GIT).unwrap();
    let socket = dir.path().join("run/s");

    let (out, took) = ctl(&socket, &dir.path().join("specs"));
    assert_eq!(labels(&out), ["checkout", "cherry-pick", "commit"], "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert!(took < Duration::from_secs(3), "{took:?}");

    // The daemon dies and leaves its socket file behind: the next request recovers the same way.
    kill_socket_holder(&socket);
    std::thread::sleep(Duration::from_millis(200));
    assert!(socket.exists(), "a stale socket file remains");
    let (out, _) = ctl(&socket, &dir.path().join("specs"));
    assert_eq!(labels(&out).len(), 3, "recovered from a stale socket");
    kill_socket_holder(&socket);
}

#[test]
fn a_daemon_that_cannot_start_means_no_output_and_failure_within_a_bound() {
    let dir = tempfile::Builder::new().prefix("fc").tempdir_in("/tmp").unwrap();
    let (out, took) = ctl(Path::new("/dev/null/nope/s"), dir.path());
    assert!(!out.status.success() && out.stdout.is_empty());
    assert!(took < Duration::from_secs(3), "{took:?}");
}

fn run_ctl(args: &[&str], socket: &Path, specs: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_goh"))
        .args(args)
        .env("GOH_SOCKET", socket)
        .env("GOH_SPEC_DIR", specs)
        .env("GOH_CONFIG", "/nonexistent")
        .output()
        .unwrap()
}

#[test]
fn the_common_commands_work_with_no_setup_at_all() {
    // An empty spec directory: nothing installed by the user. git, docker and npm still complete.
    let dir = tempfile::Builder::new().prefix("fc").tempdir_in("/tmp").unwrap();
    let specs = dir.path().join("specs");
    fs::create_dir(&specs).unwrap();
    let socket = dir.path().join("run/s");
    let ask = |buf: &str| -> Vec<String> {
        let out = run_ctl(
            &["menu", "--session", "test-session-2", "--cwd", "/", "--cursor-byte", &buf.len().to_string(), "--buffer", buf],
            &socket,
            &specs,
        );
        labels(&out)
    };
    let git = ask("git c");
    assert!(["checkout", "cherry-pick", "clean", "clone", "commit"].iter().all(|l| git.contains(&l.to_string())), "{git:?}");
    assert!(ask("docker r").contains(&"run".to_string()));
    assert!(ask("npm in").contains(&"install".to_string()));
    assert!(ask("pnpm a").contains(&"add".to_string()));
    assert!(ask("brew in").contains(&"install".to_string()));
    assert!(ask("aws s3 c").contains(&"cp".to_string()));
    kill_socket_holder(&socket);
}

#[test]
fn specs_command_says_where_each_comes_from() {
    let dir = tempfile::Builder::new().prefix("fc").tempdir_in("/tmp").unwrap();
    let specs = dir.path().join("specs");
    fs::create_dir(&specs).unwrap();
    fs::write(specs.join("git.json"), common::GIT).unwrap();
    let out = run_ctl(&["specs"], &dir.path().join("none"), &specs);
    assert!(out.status.success());
    let notice = String::from_utf8_lossy(&out.stderr);
    assert!(notice.contains("next daemon start") && notice.contains("running daemon may differ"), "{notice}");
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    let rows: Vec<Vec<&str>> = text.lines().map(|l| l.split('\t').collect()).collect();
    let git = rows.iter().find(|r| r[0] == "git").expect("git is listed");
    assert!(git[1].starts_with("yours:") && git[1].ends_with("git.json"), "{git:?}");
    let bundled = goh_core::bundled_specs().into_iter().find(|b| b.name == "git").unwrap().subcommands.len().to_string();
    assert_eq!((git[2], git[3]), ("3", bundled.as_str()), "yours has 3 subcommands and replaces the bundled one");
    for name in ["docker", "npm"] {
        let r = rows.iter().find(|r| r[0] == name).unwrap_or_else(|| panic!("{name} is listed"));
        assert_eq!((r[1], r[3]), ("bundled", ""), "{r:?}");
    }
}

#[test]
fn utility_commands_start_reload_and_stop_the_daemon() {
    let dir = tempfile::Builder::new().prefix("fc").tempdir_in("/tmp").unwrap();
    fs::create_dir(dir.path().join("specs")).unwrap();
    let (socket, specs) = (dir.path().join("run/s"), dir.path().join("specs"));
    let run = |args: &[&str]| run_ctl(args, &socket, &specs);
    let text = |o: &Output| String::from_utf8_lossy(&o.stdout).into_owned();

    assert!(text(&run(&["status"])).contains("not running"));
    assert!(run(&["start"]).status.success());
    assert!(text(&run(&["status"])).contains("running (pid"));
    let reload = run(&["reload"]);
    assert!(reload.status.success() && text(&reload).contains("reloaded"), "{reload:?}");
    assert!(text(&run(&["stop"])).contains("stopped gohd"));
    assert!(text(&run(&["status"])).contains("not running"));
    assert!(text(&run(&["stop"])).contains("was not running"), "stopping twice is fine");

    assert!(text(&run(&["version"])).starts_with("goh "));
    assert!(text(&run(&["help"])).contains("reload"));
    assert!(text(&run(&["init", "zsh"])).contains("goh.zsh"));
    assert!(!run(&["init", "nope"]).status.success());
    assert!(!run(&["bogus"]).status.success());
}

#[test]
fn config_creates_the_file_opens_the_editor_and_feeds_the_adapters() {
    let dir = tempfile::Builder::new().prefix("fc").tempdir_in("/tmp").unwrap();
    let file = dir.path().join("conf/config.json");
    let run = |args: &[&str], editor: &str| {
        Command::new(env!("CARGO_BIN_EXE_goh")).args(args).env("GOH_CONFIG", &file).env("EDITOR", editor).env_remove("VISUAL").output().unwrap()
    };
    let text = |o: &Output| String::from_utf8_lossy(&o.stdout).into_owned();

    assert!(text(&run(&["config", "show"], "true")).contains("no config file"));
    assert!(run(&["config", "env"], "true").stdout.is_empty(), "no file, no settings: the adapters keep their defaults");

    // The editor is run with the file's path; `sh -c` lets EDITOR carry arguments.
    let marker = dir.path().join("opened");
    let editor = format!("sh -c 'echo \"$0\" > {}' ", marker.display());
    let out = run(&["config"], &editor);
    assert!(out.status.success(), "{out:?}");
    assert!(file.exists(), "created from the template");
    assert_eq!(fs::read_to_string(&marker).unwrap().trim(), file.to_str().unwrap());
    assert!(text(&run(&["config", "env"], "true")).contains("GOH_LIMIT=8"));

    // A file edited into something unusable is reported, and the command fails.
    fs::write(&file, r#"{"display":{"mode":"popup"}}"#).unwrap();
    let out = run(&["config"], "true");
    assert!(!out.status.success() && String::from_utf8_lossy(&out.stderr).contains("display.mode"), "{out:?}");
    assert!(run(&["config", "env"], "true").stdout.is_empty(), "an unusable value never reaches the adapters");
    assert_eq!(text(&run(&["config", "path"], "true")).trim(), file.to_str().unwrap());
}

#[test]
fn the_shells_aliases_and_commands_reach_the_menu() {
    use std::io::Write;
    use std::process::Stdio;
    let dir = tempfile::Builder::new().prefix("fc").tempdir_in("/tmp").unwrap();
    fs::create_dir(dir.path().join("specs")).unwrap();
    fs::write(dir.path().join("specs/git.json"), common::GIT).unwrap();
    let bin = dir.path().join("bin");
    fs::create_dir(&bin).unwrap();
    fs::write(bin.join("gadget"), "#!/bin/sh\n").unwrap();
    fs::set_permissions(bin.join("gadget"), std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    let (socket, specs) = (dir.path().join("run/s"), dir.path().join("specs"));
    let session = "test-session-ctx";

    let mut child = Command::new(env!("CARGO_BIN_EXE_goh"))
        .args(["context", "--session", session])
        .env("GOH_SOCKET", &socket)
        .env("GOH_SPEC_DIR", &specs)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .spawn()
        .unwrap();
    write!(child.stdin.take().unwrap(), "P\t{}\nA\tg\tgit\nF\tmkcd\nB\tcd\n", bin.display()).unwrap();
    assert!(child.wait().unwrap().success(), "context is accepted (the daemon starts if needed)");

    let menu = |buffer: &str| {
        let out = Command::new(env!("CARGO_BIN_EXE_goh"))
            .args(["menu", "--session", session, "--cwd", "/", "--cursor-byte", &buffer.len().to_string(), "--buffer", buffer])
            .env("GOH_SOCKET", &socket)
            .env("GOH_SPEC_DIR", &specs)
            .env("GOH_CONFIG", "/nonexistent")
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout).lines().map(|l| l.split('\x1f').map(str::to_string).collect::<Vec<_>>()).collect::<Vec<_>>()
    };
    // `g ch` completes as `git ch`, and the edit is still expressed in the line as typed.
    let rows = menu("g ch");
    assert_eq!(rows[0][4], "checkout", "{rows:?}");
    assert_eq!((rows[0][0].as_str(), rows[0][1].as_str(), rows[0][5].as_str()), ("2", "4", "checkout "));
    // As-you-type asks the same way, with `--live`: an alias must not be what makes it stay silent.
    let live = Command::new(env!("CARGO_BIN_EXE_goh"))
        .args(["menu", "--live", "--session", session, "--cwd", "/", "--cursor-byte", "3", "--buffer", "g c"])
        .env("GOH_SOCKET", &socket)
        .env("GOH_SPEC_DIR", &specs)
        .env("GOH_CONFIG", "/nonexistent")
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&live.stdout).contains("checkout"), "live: {live:?}");
    // The command word completes from the shell's own names.
    let names: Vec<String> = menu("gad").into_iter().map(|r| r[4].clone()).collect();
    assert!(names.contains(&"gadget".to_string()), "{names:?}");
    let names: Vec<String> = menu("mk").into_iter().map(|r| r[4].clone()).collect();
    assert_eq!(names[0], "mkcd", "a function you defined is offered first: {names:?}");
    kill_socket_holder(&socket);
}

#[test]
fn what_you_pick_comes_first_next_time_and_forget_clears_it() {
    let dir = tempfile::Builder::new().prefix("fc").tempdir_in("/tmp").unwrap();
    fs::create_dir(dir.path().join("specs")).unwrap();
    fs::write(dir.path().join("specs/git.json"), common::GIT).unwrap();
    let (socket, specs, usage) = (dir.path().join("run/s"), dir.path().join("specs"), dir.path().join("data/usage"));
    let goh = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_goh"))
            .args(args)
            .env("GOH_SOCKET", &socket)
            .env("GOH_SPEC_DIR", &specs)
            .env("GOH_USAGE", &usage)
            .env("GOH_CONFIG", "/nonexistent")
            .output()
            .unwrap()
    };
    let order = || {
        let o = goh(&["menu", "--session", "test-session-use", "--cwd", "/", "--cursor-byte", "5", "--buffer", "git c"]);
        labels(&o)
    };
    assert_eq!(order(), ["checkout", "cherry-pick", "commit"], "spec order before anything is learned");
    for _ in 0..2 {
        assert!(goh(&["learn", "--cwd", "/", "--prefix", "git ", "--label", "commit"]).status.success());
    }
    std::thread::sleep(std::time::Duration::from_millis(2300)); // the daemon re-reads usage every couple of seconds
    assert_eq!(order(), ["commit", "checkout", "cherry-pick"], "the pick leads, the rest keep their order");
    assert!(String::from_utf8_lossy(&goh(&["forget"]).stdout).contains("forgot"));
    assert!(!usage.exists());
    kill_socket_holder(&socket);
}

#[test]
fn a_masked_secret_goes_only_to_an_adapter_that_shows_labels() {
    let dir = tempfile::Builder::new().prefix("fc").tempdir_in("/tmp").unwrap();
    fs::create_dir(dir.path().join("specs")).unwrap();
    fs::write(dir.path().join("specs/git.json"), common::GIT).unwrap();
    let (socket, specs, history) = (dir.path().join("run/s"), dir.path().join("specs"), dir.path().join("zsh_history"));
    fs::write(&history, ": 100:0;git push https://me:hunter2@example.com/r.git main\n").unwrap();
    let menu = |labels: bool| {
        let mut args = vec!["menu", "--session", "test-session-secret", "--cwd", "/", "--cursor-byte", "6", "--buffer", "git pu"];
        if labels {
            args.insert(1, "--labels");
        }
        let out = Command::new(env!("CARGO_BIN_EXE_goh"))
            .args(&args)
            .env("GOH_SOCKET", &socket)
            .env("GOH_SPEC_DIR", &specs)
            .env("GOH_CONFIG", "/nonexistent")
            .env("GOH_RECORD", "on")
            .env("GOH_RUNS", dir.path().join("runs"))
            .env("GOH_SHELL_HISTORY", &history)
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout).lines().map(|l| l.split('\x1f').map(str::to_string).collect::<Vec<_>>()).filter(|r| r[3] == "history").collect::<Vec<_>>()
    };
    let rows = menu(true);
    assert_eq!(rows.iter().map(|r| (r[4].as_str(), r[5].as_str())).collect::<Vec<_>>(), [("git push https://me:****@example.com/r.git main", "git push https://me:hunter2@example.com/r.git main")]);
    assert!(menu(false).is_empty(), "an adapter that shows what it inserts would show the secret");
    kill_socket_holder(&socket);
}

#[test]
fn spec_gen_reads_a_commands_help_and_refuses_to_clobber() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::Builder::new().prefix("fc").tempdir_in("/tmp").unwrap();
    let bin = dir.path().join("bin");
    fs::create_dir(&bin).unwrap();
    let tool = bin.join("fancytool");
    fs::write(&tool, "#!/bin/sh\ncat <<'EOF'\nA fancy tool\n\nUsage: fancytool [OPTIONS] <COMMAND>\n\nCommands:\n  build  Compile it\n  serve  Run it\n\nOptions:\n  -q, --quiet        Be quiet\n  -o, --out <FILE>   Where to write\nEOF\n").unwrap();
    fs::set_permissions(&tool, fs::Permissions::from_mode(0o755)).unwrap();
    let specs = dir.path().join("specs");
    let path = format!("{}:{}", bin.display(), std::env::var("PATH").unwrap_or_default());
    let goh = |args: &[&str]| Command::new(env!("CARGO_BIN_EXE_goh")).args(args).env("PATH", &path).env("GOH_SPEC_DIR", &specs).output().unwrap();
    let text = |o: &Output| String::from_utf8_lossy(&o.stdout).into_owned();

    let printed = goh(&["spec", "gen", "fancytool", "--print"]);
    assert!(printed.status.success() && text(&printed).contains("\"--out\"") && !specs.exists(), "--print writes nothing");
    let out = goh(&["spec", "gen", "fancytool"]);
    assert!(out.status.success() && text(&out).contains("4 options, 2 subcommands"), "{out:?}");
    let written: serde_json::Value = serde_json::from_str(&fs::read_to_string(specs.join("fancytool.json")).unwrap()).unwrap();
    assert_eq!(written["subcommands"][1]["name"], "serve");
    assert!(!goh(&["spec", "gen", "fancytool"]).status.success(), "an existing spec is not overwritten");
    assert!(goh(&["spec", "gen", "fancytool", "--force"]).status.success());
    assert!(!goh(&["spec", "gen", "git"]).status.success(), "a bundled spec is not replaced by a worse one");
    assert!(!goh(&["spec", "gen", "../evil"]).status.success() && !goh(&["spec", "gen", "no-such-command-xyz"]).status.success());
}

#[test]
fn menu_late_prints_pending_while_docker_has_not_answered() {
    let dir = tempfile::Builder::new().prefix("fl").tempdir_in("/tmp").unwrap();
    let (socket, specs, bin) = (dir.path().join("run/s"), dir.path().join("specs"), dir.path().join("bin"));
    fs::create_dir_all(&specs).unwrap();
    fs::create_dir_all(&bin).unwrap();
    let docker = bin.join("docker");
    fs::write(&docker, "#!/bin/sh\nsleep 1\necho '{\"Names\":\"web-1\",\"Image\":\"nginx\"}'\n").unwrap();
    fs::set_permissions(&docker, std::os::unix::fs::PermissionsExt::from_mode(0o700)).unwrap();
    let goh_dir = Path::new(env!("CARGO_BIN_EXE_goh")).parent().unwrap();
    let path = format!("{}:{}:/usr/bin:/bin", bin.display(), goh_dir.display());
    let run = |late: bool| {
        let mut args = vec!["menu", "--session", "test-late-1", "--cwd", "/", "--cursor-byte", "12", "--buffer", "docker exec "];
        if late {
            args.push("--late");
        }
        let out = Command::new(env!("CARGO_BIN_EXE_goh"))
            .args(&args)
            .env("GOH_SOCKET", &socket)
            .env("GOH_SPEC_DIR", &specs)
            .env("GOH_CONFIG", "/nonexistent")
            .env("PATH", &path)
            .output()
            .unwrap();
        String::from_utf8(out.stdout).unwrap()
    };
    let first = run(true);
    let plain = run(false);
    std::thread::sleep(Duration::from_millis(1500));
    let later = run(true);
    // Stop the daemon before asserting, so a failure leaves none running.
    kill_socket_holder(&socket);
    assert_eq!(first.lines().last(), Some("pending"), "{first}");
    assert!(!plain.contains("pending"), "only --late prints it");
    assert!(later.contains("web-1") && !later.contains("pending"), "{later}");
}
