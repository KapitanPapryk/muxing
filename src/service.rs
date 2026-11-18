use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result, bail};

use crate::config::home_dir;

const UNIT_NAME: &str = "muxing.service";

fn unit_path() -> Result<PathBuf> {
    let base = match env::var_os("XDG_CONFIG_HOME").filter(|v| !v.is_empty()) {
        Some(dir) => PathBuf::from(dir),
        None => home_dir()?.join(".config"),
    };
    Ok(base.join("systemd").join("user").join(UNIT_NAME))
}

pub fn unit_contents(exe: &Path) -> String {
    let exe = exe.display();
    format!(
        "[Unit]
Description=muxing: start tmux sessions marked autostart
After=default.target

[Service]
Type=oneshot
RemainAfterExit=yes
ExecStart={exe} autostart
ExecStop={exe} autostop
# Don't let systemd kill the tmux server; ExecStop removes our sessions.
KillMode=process

[Install]
WantedBy=default.target
"
    )
}

fn systemctl(args: &[&str]) -> Result<()> {
    let status = Command::new("systemctl")
        .arg("--user")
        .args(args)
        .status()
        .context("failed to run systemctl")?;
    if !status.success() {
        bail!("systemctl --user {} failed", args.join(" "));
    }
    Ok(())
}

pub fn install(now: bool) -> Result<()> {
    let exe = env::current_exe()
        .and_then(fs::canonicalize)
        .context("cannot determine path of the muxing binary")?;
    let path = unit_path()?;
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    }
    fs::write(&path, unit_contents(&exe)).with_context(|| format!("writing {}", path.display()))?;
    println!("wrote {}", path.display());

    systemctl(&["daemon-reload"])?;
    if now {
        systemctl(&["enable", "--now", UNIT_NAME])?;
    } else {
        systemctl(&["enable", UNIT_NAME])?;
    }
    println!("enabled {UNIT_NAME}");

    if !linger_enabled() {
        println!(
            "note: lingering is off, so the service only starts when you log in.\n      \
             To start it at boot run: loginctl enable-linger $USER"
        );
    }
    Ok(())
}

pub fn uninstall() -> Result<()> {
    let path = unit_path()?;
    if path.exists() {
        // Stops it too (which runs `muxing autostop`).
        let _ = systemctl(&["disable", "--now", UNIT_NAME]);
        fs::remove_file(&path).with_context(|| format!("removing {}", path.display()))?;
        systemctl(&["daemon-reload"])?;
        println!("removed {}", path.display());
    } else {
        println!("{UNIT_NAME} is not installed ({} missing)", path.display());
    }
    Ok(())
}

pub fn status() -> Result<()> {
    let _ = Command::new("systemctl")
        .args(["--user", "status", "--no-pager", UNIT_NAME])
        .status()
        .context("failed to run systemctl")?;
    println!(
        "\nlinger: {}",
        if linger_enabled() { "enabled (starts at boot)" } else { "disabled (starts at login)" }
    );
    Ok(())
}

fn linger_enabled() -> bool {
    let user = env::var("USER").unwrap_or_default();
    Command::new("loginctl")
        .args(["show-user", &user, "--property=Linger", "--value"])
        .output()
        .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).trim() == "yes")
}
