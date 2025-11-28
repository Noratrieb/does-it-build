use std::path::PathBuf;

use sha2::Digest;

fn main() {
    // Always rerun.

    let index_css = include_str!("static/index.css");
    let index_js = include_str!("static/index.js");

    let index_css_name = get_file_ref("css", index_css);
    let index_js_name = get_file_ref("js", index_js);

    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());

    std::fs::write(
        out_dir.join("revs.rs"),
        format!(
            r#"
pub const INDEX_CSS_NAME: &str = "{index_css_name}";
pub const INDEX_JS_NAME: &str = "{index_js_name}";
    "#
        ),
    )
    .unwrap();

    let version = if let Ok(commit) = try_get_commit() {
        match has_no_changes() {
            Ok(true) => commit,
            Ok(false) => format!("{commit} (*)"),
            Err(_) => format!("{commit} (?)"),
        }
    } else {
        "unknown".into()
    };

    println!("cargo:rustc-env=GIT_COMMIT={version}");
    let version_short = if version.len() > 16 {
        &version[..16]
    } else {
        &version
    };
    println!("cargo:rustc-env=GIT_COMMIT_SHORT={version_short}");
}

fn get_file_ref(ext: &str, content: &str) -> String {
    let mut hash = sha2::Sha256::new();
    hash.update(content);
    let digest = hash.finalize();
    let rev = bs58::encode(digest).into_string();
    let rev = &rev[..16];
    format!("/{rev}.{ext}")
}

fn try_get_commit() -> color_eyre::Result<String> {
    if let Ok(overridden) = std::env::var("DOES_IT_BUILD_OVERRIDE_VERSION") {
        return Ok(overridden);
    }

    let stdout = std::process::Command::new("git")
        .arg("rev-parse")
        .arg("HEAD")
        .output()?
        .stdout;

    let stdout = String::from_utf8(stdout)?;

    Ok(stdout.trim()[0..8].to_owned())
}

fn has_no_changes() -> color_eyre::Result<bool> {
    if std::env::var("DOES_IT_BUILD_OVERRIDE_VERSION").is_ok() {
        return Ok(true);
    }

    Ok(std::process::Command::new("git")
        .args(["diff", "--no-ext-diff", "--quiet", "--exit-code"])
        .output()?
        .status
        .success())
}
