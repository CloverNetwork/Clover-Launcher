//! Commands someone sets to run before an instance launches and after it exits, such as
//! syncing a world or starting a voice app. They run in the instance's game folder, with the
//! same environment variables Prism Launcher and MultiMC provide, so existing scripts work.

use std::path::Path;

use crate::error::{Result, msg};
use crate::instances::Instance;
use crate::paths::Paths;

fn shell(command: &str) -> tokio::process::Command {
    if cfg!(windows) {
        let mut c = tokio::process::Command::new("cmd");
        c.arg("/C").arg(command);
        c
    } else {
        let mut c = tokio::process::Command::new("sh");
        c.arg("-c").arg(command);
        c
    }
}

/// Runs a hook and waits for it. An empty command does nothing.
pub async fn run(paths: &Paths, instance: &Instance, command: &str, java: Option<&Path>) -> Result<()> {
    let command = command.trim();
    if command.is_empty() {
        return Ok(());
    }
    let instance_dir = paths.instance(&instance.id);
    let game_dir = instance_dir.join("minecraft");
    std::fs::create_dir_all(&game_dir)?;
    let mut process = shell(command);
    process
        .current_dir(&game_dir)
        .env("INST_ID", &instance.id)
        .env("INST_NAME", &instance.name)
        .env("INST_DIR", &instance_dir)
        .env("INST_MC_DIR", &game_dir)
        .env("INST_MC_VERSION", &instance.version);
    if let Some(java) = java {
        process.env("INST_JAVA", java);
    }
    let status = process.status().await.map_err(|e| msg(format!("Couldn't run the command ({e}).")))?;
    if !status.success() {
        let code = status.code().map_or("a signal".into(), |c| format!("code {c}"));
        return Err(msg(format!("The command exited with {code}.")));
    }
    Ok(())
}

/// Splits JVM arguments the way a shell would: spaces separate, quotes group.
pub fn split_args(text: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut started = false;
    for c in text.chars() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), c) => current.push(c),
            (None, '"' | '\'') => {
                quote = Some(c);
                started = true;
            }
            (None, c) if c.is_whitespace() => {
                if started || !current.is_empty() {
                    args.push(std::mem::take(&mut current));
                    started = false;
                }
            }
            (None, c) => current.push(c),
        }
    }
    if started || !current.is_empty() {
        args.push(current);
    }
    args
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_like_a_shell() {
        assert_eq!(split_args(r#"-XX:+UseG1GC  -Dname="My World" '-Dx=a b'"#), ["-XX:+UseG1GC", "-Dname=My World", "-Dx=a b"]);
        assert!(split_args("   ").is_empty());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn runs_hooks_with_instance_variables() {
        let root = std::env::temp_dir().join(format!("clover-hooks-{}", std::process::id()));
        let paths = Paths::at(root.clone());
        let instance = crate::instances::create(&paths, "Hooked", "1.21.4", "vanilla", None).unwrap();
        run(&paths, &instance, r#"echo "$INST_NAME $INST_MC_VERSION" > hook.txt"#, None).await.unwrap();
        let written = std::fs::read_to_string(paths.instance(&instance.id).join("minecraft/hook.txt")).unwrap();
        assert_eq!(written.trim(), "Hooked 1.21.4");
        assert!(run(&paths, &instance, "exit 3", None).await.unwrap_err().to_string().contains("code 3"));
        std::fs::remove_dir_all(root).unwrap();
    }
}
