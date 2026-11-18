use std::{
    env,
    ffi::OsStr,
    fs::File,
    os::unix::process::CommandExt,
    process::{Command, Stdio},
};

use anyhow::{Context, Result, bail};

use crate::config::{ResolvedWindow, Session};

fn tmux_bin() -> String {
    env::var("MUXING_TMUX")
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "tmux".to_string())
}

fn command<I, S>(args: I) -> Command
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut cmd = Command::new(tmux_bin());
    cmd.args(args);
    cmd
}

/// Run tmux and return trimmed stdout, failing with tmux's stderr.
fn run<I, S>(args: I) -> Result<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let args: Vec<_> = args.into_iter().map(|a| a.as_ref().to_os_string()).collect();
    let out = command(&args)
        .stdin(Stdio::null())
        .output()
        .with_context(|| format!("failed to run {}", tmux_bin()))?;
    if !out.status.success() {
        let printable: Vec<_> = args.iter().map(|a| a.to_string_lossy()).collect();
        bail!(
            "tmux {} failed: {}",
            printable.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

pub fn has_session(name: &str) -> bool {
    command(["has-session", "-t", &format!("={name}")])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

pub fn kill_session(name: &str) -> Result<()> {
    run(["kill-session", "-t", &format!("={name}")]).map(|_| ())
}

/// Create the session if it doesn't exist. Returns `true` if it was created.
pub fn ensure_session(session: &Session) -> Result<bool> {
    if has_session(&session.name) {
        return Ok(false);
    }
    if let Err(err) = build(session) {
        if has_session(&session.name) {
            let _ = kill_session(&session.name);
        }
        return Err(err.context(format!("building session '{}'", session.name)));
    }
    Ok(true)
}

fn build(session: &Session) -> Result<()> {
    let mut first_window = None;
    for (i, window) in session.windows.iter().enumerate() {
        let first_pane = &window.panes[0];
        let root = first_pane.root.to_string_lossy();
        let mut args: Vec<String> = if i == 0 {
            let (cols, lines) = initial_size();
            vec![
                "new-session".into(),
                "-d".into(),
                "-s".into(),
                session.name.clone(),
                "-x".into(),
                cols.to_string(),
                "-y".into(),
                lines.to_string(),
            ]
        } else {
            vec!["new-window".into(), "-d".into(), "-t".into(), format!("={}:", session.name)]
        };
        if let Some(name) = &window.name {
            args.extend(["-n".into(), name.clone()]);
        }
        args.extend([
            "-c".into(),
            root.into_owned(),
            "-P".into(),
            "-F".into(),
            "#{window_id} #{pane_id}".into(),
        ]);
        let ids = run(&args)?;
        let (window_id, pane_id) = ids
            .split_once(' ')
            .with_context(|| format!("unexpected tmux output '{ids}'"))?;
        let window_id = window_id.to_string();
        if first_window.is_none() {
            first_window = Some(window_id.clone());
        }
        build_panes(window, &window_id, pane_id)?;
    }
    if let Some(window_id) = first_window {
        run(["select-window", "-t", &window_id])?;
    }
    Ok(())
}

fn build_panes(window: &ResolvedWindow, window_id: &str, first_pane_id: &str) -> Result<()> {
    let mut pane_ids = vec![first_pane_id.to_string()];
    for pane in &window.panes[1..] {
        // Split the most recently created pane so panes keep their config order.
        let last = pane_ids.last().expect("at least one pane").clone();
        let id = run([
            "split-window",
            "-d",
            "-t",
            &last,
            "-c",
            &pane.root.to_string_lossy(),
            "-P",
            "-F",
            "#{pane_id}",
        ])?;
        // Re-tile after every split so detached (small) windows don't run out of room.
        run(["select-layout", "-t", window_id, "tiled"])?;
        pane_ids.push(id);
    }

    match &window.layout {
        Some(layout) => run(["select-layout", "-t", window_id, layout]).map(|_| ())?,
        None if pane_ids.len() > 1 => run(["select-layout", "-t", window_id, "tiled"]).map(|_| ())?,
        None => {}
    }

    for (pane, id) in window.panes.iter().zip(&pane_ids) {
        for cmd in &pane.commands {
            run(["send-keys", "-t", id, "-l", cmd])?;
            run(["send-keys", "-t", id, "Enter"])?;
        }
    }
    run(["select-pane", "-t", &pane_ids[0]])?;
    Ok(())
}

/// Size for a new detached session: the current terminal's, or a roomy default
/// (e.g. under systemd) so layouts like main-vertical have space before a client attaches.
fn initial_size() -> (u16, u16) {
    let from_tty = File::open("/dev/tty").ok().and_then(|tty| {
        let out = Command::new("stty").arg("size").stdin(tty).output().ok()?;
        let text = String::from_utf8_lossy(&out.stdout);
        let mut it = text.split_whitespace().map(|n| n.parse::<u16>().ok());
        let (lines, cols) = (it.next()??, it.next()??);
        (cols > 0 && lines > 0).then_some((cols, lines))
    });
    from_tty.unwrap_or((200, 50))
}

/// Attach to (or switch to, when already inside tmux) the given session.
pub fn attach(name: &str) -> Result<()> {
    let target = format!("={name}");
    if env::var_os("TMUX").is_some_and(|v| !v.is_empty()) {
        run(["switch-client", "-t", &target]).map(|_| ())
    } else {
        let err = command(["attach-session", "-t", &target]).exec();
        Err(err).context("failed to exec tmux attach-session")
    }
}
