fn main() {
    // Always rerun.

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
}

fn try_get_commit() -> color_eyre::Result<String> {
    let stdout = std::process::Command::new("git")
        .arg("rev-parse")
        .arg("HEAD")
        .output()?
        .stdout;

    let stdout = String::from_utf8(stdout)?;

    Ok(stdout.trim()[0..8].to_owned())
}

fn has_no_changes() -> color_eyre::Result<bool> {
    Ok(std::process::Command::new("git")
        .args(["diff", "--no-ext-diff", "--quiet", "--exit-code"])
        .output()?
        .status
        .success())
}
