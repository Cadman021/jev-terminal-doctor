use std::path::Path;

/// نوع پروژه را از روی وجود فایل‌های مشخصه در ریشه‌ی مسیر تشخیص می‌دهد.
/// این خیلی حیاتی است چون تشخیص خطا (errors::detector) و انتخاب پرامپت
/// مناسب برای AI به تولچین بستگی دارد.
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
