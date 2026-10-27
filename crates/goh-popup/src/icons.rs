//! Pinned local command logos. No font glyphs, remote URLs or spec-provided asset paths.
pub const ASSETS: &[(&str, &[u8])] = &[
    ("docker", include_bytes!("../assets/docker.png")),
    ("nodejs", include_bytes!("../assets/nodejs.png")),
    ("go", include_bytes!("../assets/go.png")),
    ("git", include_bytes!("../assets/git.png")),
    ("npm", include_bytes!("../assets/npm.png")),
    ("python", include_bytes!("../assets/python.png")),
    ("rust", include_bytes!("../assets/rust.png")),
    ("kubernetes", include_bytes!("../assets/kubernetes.png")),
    ("pnpm", include_bytes!("../assets/pnpm.png")),
    ("yarn", include_bytes!("../assets/yarn.png")),
    ("terraform", include_bytes!("../assets/terraform.png")),
    ("github", include_bytes!("../assets/github.png")),
];

fn command(name: &str) -> Option<&'static str> {
    match name {
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

pub fn key(context: &str, label: &str) -> Option<&'static str> {
    command(context)
        .or_else(|| command(label))
        .or_else(|| (!context.is_empty() || !label.is_empty()).then_some("terminal"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn aliases_share_logos_and_subcommands_keep_their_parent_brand() {
        assert_eq!(key("docker", "go"), Some("docker"));
        assert_eq!(key("node", "--inspect"), Some("nodejs"));
        assert_eq!(key("", "go"), Some("go"));
        assert_eq!(key("python3", "script.py"), Some("python"));
        assert_eq!(key("custom", "unknown"), Some("terminal"));
        assert_eq!(key("", "custom-command"), Some("terminal"));
        assert_eq!(key("custom", ""), Some("terminal"));
        assert_eq!(key("", ""), None);
    }
    #[test]
    fn every_logo_is_a_decodable_rgba_image() {
        for (name, bytes) in ASSETS {
            let mut r = png::Decoder::new(std::io::Cursor::new(bytes))
                .read_info()
                .unwrap();
            let mut buf = vec![0; r.output_buffer_size().unwrap()];
            let info = r.next_frame(&mut buf).unwrap();
            assert_eq!(
                (info.width, info.height, info.color_type),
                (64, 64, png::ColorType::Rgba),
                "{name}"
            );
        }
    }
}
