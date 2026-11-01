//! Which brand a command belongs to, so every surface shows the same logo for it (`icons::brand_glyph`). Aliases
//! share their brand (`npx` is npm, `helm` is Kubernetes).

pub fn brand(command: &str) -> Option<&'static str> {
    match command {
        "docker" | "docker-compose" => Some("docker"),
        "node" | "nodejs" => Some("nodejs"),
        "go" => Some("go"),
        "git" => Some("git"),
        "npm" | "npx" => Some("npm"),
        "python" | "python3" | "pip" | "pip3" | "uv" => Some("python"),
        "cargo" | "rustc" | "rustup" => Some("rust"),
        "kubectl" | "helm" => Some("kubernetes"),
        "pnpm" => Some("pnpm"),
        "yarn" => Some("yarn"),
        "terraform" => Some("terraform"),
        "gh" => Some("github"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aliases_share_a_brand_and_unknown_commands_have_none() {
        assert_eq!(brand("helm"), brand("kubectl"));
        assert_eq!(brand("python3"), Some("python"));
        assert_eq!(brand("grep"), None);
    }
}
