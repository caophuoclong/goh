//! A word of warning next to the suggestions that can do damage, and which cluster or workspace they would act on.
//! `kubectl delete` in the wrong context is the classic. The note is part of the suggestion's description:
//! nothing is blocked and nothing is run.

use goh_core::protocol::{Candidate, Kind};

/// Where the thing a command acts on is written down.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Target {
    None,
    /// The current kubectl context.
    Kube,
    /// The current Terraform workspace.
    Terraform,
}

/// (command words before it, the suggestion, what to say, where it acts)
const RISKS: &[(&str, &str, &str, Target)] = &[
    ("git push", "--force", "overwrites the remote branch", Target::None),
    ("git push", "-f", "overwrites the remote branch", Target::None),
    ("git push", "--delete", "deletes the remote branch or tag", Target::None),
    ("git reset", "--hard", "discards uncommitted changes", Target::None),
    ("git clean", "-f", "deletes untracked files for good", Target::None),
    ("git clean", "--force", "deletes untracked files for good", Target::None),
    ("git branch", "-D", "deletes the branch even if unmerged", Target::None),
    ("git stash", "drop", "deletes the stash", Target::None),
    ("git stash", "clear", "deletes every stash", Target::None),
    ("rm", "-r", "deletes folders, with no undo", Target::None),
    ("rm", "-R", "deletes folders, with no undo", Target::None),
    ("rm", "--recursive", "deletes folders, with no undo", Target::None),
    ("rm", "-f", "never asks first", Target::None),
    ("rm", "--force", "never asks first", Target::None),
    ("docker system", "prune", "removes unused containers, networks and images", Target::None),
    ("docker volume", "prune", "deletes unused volumes and their data", Target::None),
    ("kubectl", "delete", "deletes cluster resources", Target::Kube),
    ("kubectl", "apply", "changes the cluster", Target::Kube),
    ("kubectl", "replace", "changes the cluster", Target::Kube),
    ("kubectl", "patch", "changes the cluster", Target::Kube),
    ("kubectl", "scale", "changes the cluster", Target::Kube),
    ("kubectl", "drain", "evicts the pods of a node", Target::Kube),
    ("kubectl", "rollout", "changes a deployment", Target::Kube),
    ("helm", "install", "changes the cluster", Target::Kube),
    ("helm", "upgrade", "changes the cluster", Target::Kube),
    ("helm", "uninstall", "removes a release", Target::Kube),
    ("helm", "rollback", "changes the cluster", Target::Kube),
    ("terraform", "apply", "changes real infrastructure", Target::Terraform),
    ("terraform", "destroy", "destroys real infrastructure", Target::Terraform),
    ("git push", "--force-with-lease", "overwrites the remote branch", Target::None),
    ("git push", "--mirror", "makes the remote match, deleting what it lacks", Target::None),
    ("git push", "--prune", "deletes remote branches you do not have", Target::None),
    ("git checkout", "-f", "discards local changes", Target::None),
    ("git checkout", "--force", "discards local changes", Target::None),
    ("git tag", "-d", "deletes the tag", Target::None),
    ("git tag", "--delete", "deletes the tag", Target::None),
    ("git reflog", "expire", "drops what recovers lost commits", Target::None),
    ("git gc", "--prune", "deletes unreachable commits for good", Target::None),
    ("git", "filter-branch", "rewrites the whole history", Target::None),
    ("docker", "rm", "deletes containers", Target::None),
    ("docker", "rmi", "deletes images", Target::None),
    ("docker container", "rm", "deletes containers", Target::None),
    ("docker container", "prune", "removes stopped containers", Target::None),
    ("docker image", "rm", "deletes images", Target::None),
    ("docker image", "prune", "removes unused images", Target::None),
    ("docker volume", "rm", "deletes volumes and their data", Target::None),
    ("docker network", "prune", "removes unused networks", Target::None),
    ("docker builder", "prune", "clears the build cache", Target::None),
    ("docker system prune", "--volumes", "also deletes unused volumes and their data", Target::None),
    ("docker system prune", "-a", "removes every unused image too", Target::None),
    ("docker system prune", "--all", "removes every unused image too", Target::None),
    ("docker compose", "down", "stops and removes the containers", Target::None),
    ("docker compose down", "-v", "deletes the volumes and their data", Target::None),
    ("docker compose down", "--volumes", "deletes the volumes and their data", Target::None),
    ("terraform state", "rm", "Terraform stops managing these resources", Target::Terraform),
    ("terraform workspace", "delete", "deletes the workspace's state", Target::Terraform),
    ("terraform apply", "-auto-approve", "applies without asking", Target::Terraform),
    ("terraform destroy", "-auto-approve", "destroys without asking", Target::Terraform),
    // Databases.
    ("prisma migrate", "reset", "drops the database and all its data", Target::None),
    ("prisma migrate", "deploy", "changes the database schema", Target::None),
    ("prisma db push", "--force-reset", "drops the database and all its data first", Target::None),
    ("prisma db push", "--accept-data-loss", "applies changes that lose data", Target::None),
    ("prisma db", "execute", "runs SQL on the database", Target::None),
    ("supabase db", "reset", "deletes all data in the local database", Target::None),
    ("supabase db", "push", "changes the linked remote database", Target::None),
    ("rake", "db:drop", "drops the database", Target::None),
    ("rake", "db:reset", "drops the database and recreates it", Target::None),
    ("rake", "db:schema:load", "replaces the schema, deleting the data", Target::None),
    ("psql", "-c", "runs the SQL at once, with no undo", Target::None),
    ("psql", "--command", "runs the SQL at once, with no undo", Target::None),
    // Files and machines.
    ("find", "-delete", "deletes every match", Target::None),
    ("rsync", "--delete", "deletes what the source lacks at the destination", Target::None),
    ("rsync", "--delete-after", "deletes what the source lacks at the destination", Target::None),
    ("rsync", "--delete-excluded", "deletes excluded files at the destination", Target::None),
    ("rclone", "sync", "deletes what the source lacks at the destination", Target::None),
    ("rclone", "purge", "deletes the path and everything in it", Target::None),
    ("tar", "--remove-files", "deletes the files once archived", Target::None),
    ("chmod", "-R", "changes every file below", Target::None),
    ("chown", "-R", "changes every file below", Target::None),
    ("mv", "-f", "overwrites without asking", Target::None),
    ("cp", "-f", "overwrites without asking", Target::None),
    ("systemctl", "stop", "stops the service", Target::None),
    ("systemctl", "disable", "keeps the service from starting", Target::None),
    ("systemctl", "mask", "keeps the service from starting at all", Target::None),
    ("systemctl", "poweroff", "turns the machine off", Target::None),
    ("systemctl", "reboot", "restarts the machine", Target::None),
    ("apt", "purge", "removes packages and their settings", Target::None),
    ("apt", "autoremove", "removes packages nothing needs any more", Target::None),
    ("gpg", "--delete-secret-keys", "deletes secret keys for good", Target::None),
    ("gpg", "--delete-keys", "deletes keys", Target::None),
    // What others can see.
    ("npm", "publish", "publishes to the registry", Target::None),
    ("pnpm", "publish", "publishes to the registry", Target::None),
    ("yarn", "publish", "publishes to the registry", Target::None),
    ("bun", "publish", "publishes to the registry", Target::None),
    ("npm", "unpublish", "removes a published version", Target::None),
    ("cargo", "publish", "publishes to crates.io, for good", Target::None),
    ("gh repo", "delete", "deletes the repository and its issues", Target::None),
    ("gh release", "delete", "deletes the release", Target::None),
    ("aws s3", "rm", "deletes objects", Target::None),
    ("aws s3", "rb", "removes the bucket", Target::None),
    ("aws s3 rm", "--recursive", "deletes everything under the prefix", Target::None),
    ("aws s3 rb", "--force", "deletes every object first", Target::None),
    ("aws s3 sync", "--delete", "deletes what the source lacks at the destination", Target::None),
    ("serverless", "remove", "removes the service and its resources", Target::None),
];

/// Subcommands whose name says they destroy something, in any command not listed above (`gcloud ... delete`,
/// `minikube delete`, `redis-cli flushall`). git is left out: its subcommands are named differently.
const VERBS: &[(&str, &str)] = &[
    ("delete", "deletes it for good"),
    ("destroy", "destroys it for good"),
    ("drop", "deletes it for good"),
    ("purge", "deletes it for good"),
    ("wipe", "deletes it for good"),
    ("erase", "deletes it for good"),
    ("remove", "removes it"),
    ("rm", "removes it"),
    ("uninstall", "removes it"),
    ("prune", "removes what is unused"),
    ("flush", "erases its data"),
    ("flushall", "erases all data"),
    ("flushdb", "erases the database"),
    ("truncate", "erases its data"),
];

/// The warning for `label` after the command words in `before` (`git push ` + `--force`), if there is one.
fn note(before: &str, label: &str, kind: Kind) -> Option<(&'static str, Target)> {
    let words: Vec<&str> = before.split_whitespace().filter(|w| !w.starts_with('-')).collect();
    let path = words.join(" ");
    let listed = RISKS.iter().find(|(p, l, _, _)| *p == path && *l == label).map(|(_, _, text, target)| (*text, *target));
    listed.or_else(|| {
        (kind == Kind::Subcommand && words.first().is_some_and(|w| *w != "git"))
            .then(|| VERBS.iter().find(|(v, _)| *v == label).map(|(_, text)| (*text, Target::None)))
            .flatten()
    })
}

/// Put the warnings into the descriptions of `candidates`. `expanded` is the line as the engine saw it
/// (aliases replaced) and `delta` how much longer it is than the one typed; `target` says where each kind of
/// command acts (`None` when it cannot tell).
pub fn annotate(candidates: &mut [Candidate], expanded: &str, delta: isize, target: &dyn Fn(Target) -> Option<String>) {
    for c in candidates.iter_mut().filter(|c| matches!(c.kind, Kind::Subcommand | Kind::Option)) {
        let end = (c.replace_start_byte as isize + delta).max(0) as usize;
        let Some((text, at)) = expanded.get(..end).and_then(|before| note(before, &c.label, c.kind)) else { continue };
        let place = if at == Target::None { None } else { target(at) };
        let mut shown = format!("⚠ {text}");
        if let Some(place) = place {
            shown.push_str(&format!(" · {place}"));
        }
        if let Some(original) = c.description.take().filter(|d| !d.is_empty()) {
            shown.push_str(&format!(" — {original}"));
        }
        c.description = Some(shown);
    }
}

/// The Terraform workspace in use: what `.terraform/environment` says, or `default` in an initialised folder.
pub fn terraform_workspace(cwd: &std::path::Path) -> Option<String> {
    cwd.ancestors().find_map(|d| {
        let dir = d.join(".terraform");
        let name = std::fs::read_to_string(dir.join("environment")).ok().map(|t| t.trim().to_string()).filter(|t| !t.is_empty() && t.len() <= 64 && !t.chars().any(char::is_control));
        name.or_else(|| dir.is_dir().then(|| "default".to_string()))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use goh_core::protocol::Source;

    fn candidate(label: &str, kind: Kind, start: usize, description: Option<&str>) -> Candidate {
        Candidate {
            id: label.into(), label: label.into(), description: description.map(str::to_string), kind, insert_text: format!("{label} "),
            replace_start_byte: start, replace_end_byte: start, match_ranges: vec![], score: 100.0, source: Source::Bundled, cursor_offset: None, takes_value: false, used: false,
        }
    }

    #[test]
    fn dangerous_options_and_subcommands_are_flagged_with_where_they_act() {
        let acts_on = |t: Target| match t {
            Target::Kube => Some("context: prod-eu".to_string()),
            _ => None,
        };
        let mut c = [candidate("--force", Kind::Option, 9, Some("Force")), candidate("--dry-run", Kind::Option, 9, None)];
        annotate(&mut c, "git push ", 0, &acts_on);
        assert_eq!(c[0].description.as_deref(), Some("⚠ overwrites the remote branch — Force"));
        assert_eq!(c[1].description, None, "other options are left alone");

        let mut c = [candidate("delete", Kind::Subcommand, 8, Some("Delete resources")), candidate("get", Kind::Subcommand, 8, Some("Display"))];
        annotate(&mut c, "kubectl ", 0, &acts_on);
        assert_eq!(c[0].description.as_deref(), Some("⚠ deletes cluster resources · context: prod-eu — Delete resources"));
        assert_eq!(c[1].description.as_deref(), Some("Display"));
    }

    #[test]
    fn databases_and_destructive_subcommands_are_flagged() {
        let mut c = [candidate("reset", Kind::Subcommand, 15, None), candidate("status", Kind::Subcommand, 15, None)];
        annotate(&mut c, "prisma migrate ", 0, &|_| None);
        assert_eq!(c[0].description.as_deref(), Some("⚠ drops the database and all its data"));
        assert_eq!(c[1].description, None);
        // Any command's `delete`, by its name; not git's, and not an option or a file called that.
        let mut c = [candidate("delete", Kind::Subcommand, 25, Some("Delete an instance"))];
        annotate(&mut c, "gcloud compute instances ", 0, &|_| None);
        assert_eq!(c[0].description.as_deref(), Some("⚠ deletes it for good — Delete an instance"));
        let mut c = [candidate("rm", Kind::Subcommand, 4, None)];
        annotate(&mut c, "git ", 0, &|_| None);
        assert_eq!(c[0].description, None);
        let mut c = [candidate("--volumes", Kind::Option, 20, None)];
        annotate(&mut c, "docker compose down ", 0, &|_| None);
        assert_eq!(c[0].description.as_deref(), Some("⚠ deletes the volumes and their data"));
    }

    #[test]
    fn the_terraform_workspace_is_read_from_the_working_directory_upwards() {
        let d = tempfile::tempdir().unwrap();
        assert_eq!(terraform_workspace(d.path()), None);
        std::fs::create_dir_all(d.path().join(".terraform")).unwrap();
        assert_eq!(terraform_workspace(d.path()).as_deref(), Some("default"));
        std::fs::write(d.path().join(".terraform/environment"), "prod\n").unwrap();
        std::fs::create_dir_all(d.path().join("modules/x")).unwrap();
        assert_eq!(terraform_workspace(&d.path().join("modules/x")).as_deref(), Some("prod"));
    }

    #[test]
    fn an_alias_is_read_through_and_a_file_of_the_same_name_is_not_flagged() {
        let mut c = [candidate("--hard", Kind::Option, 3, None)];
        annotate(&mut c, "git reset ", 7, &|_| None); // typed `gr ` for `git reset `
        assert_eq!(c[0].description.as_deref(), Some("⚠ discards uncommitted changes"));
        let mut c = [candidate("delete", Kind::File, 8, None)];
        annotate(&mut c, "kubectl ", 0, &|_| None);
        assert_eq!(c[0].description, None);
    }
}
