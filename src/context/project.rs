use std::path::Path;

/// Detect the project kind from marker files in the root directory.
/// This matters because error detection (errors::detector) and prompt
/// selection for the AI depend on the toolchain.
pub fn detect_project_kind(root: &Path) -> Option<&'static str> {
    let markers: &[(&str, &str)] = &[
        ("Cargo.toml", "cargo"),
        ("package.json", "npm"),
        ("pyproject.toml", "python-poetry"),
        ("requirements.txt", "python-pip"),
        ("go.mod", "go"),
        ("pom.xml", "maven"),
        ("build.gradle", "gradle"),
    ];

    markers
        .iter()
        .find(|(file, _)| root.join(file).exists())
        .map(|(_, kind)| *kind)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    #[test]
    fn detects_cargo_project() {
        let dir = std::env::temp_dir().join("jev_test_cargo");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("Cargo.toml"), "[package]").unwrap();
        assert_eq!(detect_project_kind(&dir), Some("cargo"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn returns_none_for_unknown() {
        let dir: PathBuf = std::env::temp_dir().join("jev_test_unknown_xyz");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        assert_eq!(detect_project_kind(&dir), None);
        let _ = fs::remove_dir_all(&dir);
    }
}
