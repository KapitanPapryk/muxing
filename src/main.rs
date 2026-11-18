mod config;
mod service;
mod tmux;

use std::{
    env, fs,
    io::{self, Write},
    process::{Command, ExitCode},
};

use anyhow::{Context, Result, bail};
use clap::{CommandFactory, Parser, Subcommand};

#[derive(Parser)]
#[command(
    version,
    about = "Manage tmux sessions from YAML project files",
    override_usage = "muxing <PROJECT> [-d]\n       muxing <COMMAND>",
    args_conflicts_with_subcommands = true
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Cmd>,
    /// Project to start and attach to (same as `muxing start <PROJECT>`)
    project: Option<String>,
    /// Don't attach after starting
    #[arg(short, long, requires = "project")]
    detached: bool,
}

#[derive(Subcommand)]
enum Cmd {
    /// Start a project (if not running) and attach to it
    #[command(alias = "s")]
    Start {
        name: String,
        /// Don't attach after starting
        #[arg(short, long)]
        detached: bool,
    },
    /// Kill a project's tmux session
    Stop { name: String },
    /// List projects
    #[command(alias = "ls")]
    List {
        /// Print only project names, one per line (used by shell completions)
        #[arg(long)]
        names: bool,
    },
    /// Create a new project file and open it in $EDITOR
    New { name: String },
    /// Open a project file in $EDITOR
    #[command(alias = "e")]
    Edit { name: String },
    /// Delete a project file
    #[command(alias = "rm")]
    Delete {
        name: String,
        /// Don't ask for confirmation
        #[arg(short = 'y', long)]
        yes: bool,
    },
    /// Validate one project, or all of them
    Check { name: Option<String> },
    /// Start (detached) every project with `autostart: true`
    Autostart,
    /// Stop every running project with `autostart: true`
    Autostop,
    /// Manage the systemd user service
    Service {
        #[command(subcommand)]
        action: ServiceCmd,
    },
    /// Print a shell completion script (bash or fish)
    Completions { shell: Shell },
    /// Print the config directory
    Dir,
}

#[derive(Clone, Copy, clap::ValueEnum)]
enum Shell {
    Bash,
    Fish,
}

#[derive(Subcommand)]
enum ServiceCmd {
    /// Write and enable ~/.config/systemd/user/muxing.service
    Install {
        /// Also start it now
        #[arg(long)]
        now: bool,
    },
    /// Disable, stop and remove the service
    Uninstall,
    /// Show service status
    Status,
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(code) => code,
        Err(err) => {
            eprintln!("muxing: {err:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<ExitCode> {
    let command = match (cli.command, cli.project) {
        (Some(command), _) => command,
        (None, Some(name)) => Cmd::Start {
            name,
            detached: cli.detached,
        },
        (None, None) => {
            Cli::command().print_help()?;
            return Ok(ExitCode::FAILURE);
        }
    };
    match command {
        Cmd::Start { name, detached } => {
            let session = config::load(&config::find_project(&name)?)?;
            if tmux::ensure_session(&session)? && detached {
                println!("started {}", session.name);
            }
            if !detached {
                tmux::attach(&session.name)?;
            }
        }
        Cmd::Stop { name } => {
            let session = config::load(&config::find_project(&name)?)?;
            if !tmux::has_session(&session.name) {
                bail!("session '{}' is not running", session.name);
            }
            tmux::kill_session(&session.name)?;
        }
        Cmd::List { names: true } => {
            for (name, _) in config::list_projects()? {
                println!("{name}");
            }
        }
        Cmd::List { names: false } => list()?,
        Cmd::New { name } => {
            let dir = config::config_dir()?;
            fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
            if config::find_project(&name).is_ok() {
                bail!("project '{name}' already exists (use `muxing edit {name}`)");
            }
            let path = dir.join(format!("{name}.yml"));
            fs::write(&path, config::TEMPLATE)
                .with_context(|| format!("writing {}", path.display()))?;
            println!("created {}", path.display());
            open_editor(&path)?;
        }
        Cmd::Edit { name } => open_editor(&config::find_project(&name)?)?,
        Cmd::Delete { name, yes } => {
            let path = config::find_project(&name)?;
            if !yes && !confirm(&format!("delete {}?", path.display()))? {
                return Ok(ExitCode::SUCCESS);
            }
            fs::remove_file(&path).with_context(|| format!("removing {}", path.display()))?;
            println!("deleted {}", path.display());
        }
        Cmd::Check { name } => return check(name),
        Cmd::Autostart => return autostart(),
        Cmd::Autostop => return autostop(),
        Cmd::Service { action } => match action {
            ServiceCmd::Install { now } => service::install(now)?,
            ServiceCmd::Uninstall => service::uninstall()?,
            ServiceCmd::Status => service::status()?,
        },
        Cmd::Completions { shell } => print!(
            "{}",
            match shell {
                Shell::Bash => include_str!("../completions/muxing.bash"),
                Shell::Fish => include_str!("../completions/muxing.fish"),
            }
        ),
        Cmd::Dir => println!("{}", config::config_dir()?.display()),
    }
    Ok(ExitCode::SUCCESS)
}

fn list() -> Result<()> {
    let projects = config::list_projects()?;
    if projects.is_empty() {
        println!("no projects in {}", config::config_dir()?.display());
        return Ok(());
    }
    let width = projects
        .iter()
        .map(|(n, _)| n.len())
        .max()
        .unwrap_or(0)
        .max(7);
    println!("{:width$}  {:9}  STATUS", "PROJECT", "AUTOSTART");
    for (name, path) in projects {
        match config::load(&path) {
            Ok(s) => {
                let running = if tmux::has_session(&s.name) {
                    "running"
                } else {
                    "-"
                };
                let auto = if s.autostart { "yes" } else { "no" };
                println!("{name:width$}  {auto:9}  {running}");
            }
            Err(_) => println!(
                "{name:width$}  {:9}  invalid (see `muxing check {name}`)",
                "?"
            ),
        }
    }
    Ok(())
}

fn check(name: Option<String>) -> Result<ExitCode> {
    let paths = match name {
        Some(name) => vec![config::find_project(&name)?],
        None => config::list_projects()?
            .into_iter()
            .map(|(_, p)| p)
            .collect(),
    };
    let mut ok = true;
    for path in paths {
        match config::load(&path) {
            Ok(s) => {
                let panes: usize = s.windows.iter().map(|w| w.panes.len()).sum();
                println!(
                    "ok    {} (session '{}', {} windows, {} panes{})",
                    path.display(),
                    s.name,
                    s.windows.len(),
                    panes,
                    if s.autostart { ", autostart" } else { "" }
                );
            }
            Err(err) => {
                ok = false;
                println!("error {err:#}");
            }
        }
    }
    Ok(if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

fn autostart() -> Result<ExitCode> {
    let mut ok = true;
    for (name, path) in config::list_projects()? {
        let session = match config::load(&path) {
            Ok(s) => s,
            Err(err) => {
                ok = false;
                eprintln!("skipping {name}: {err:#}");
                continue;
            }
        };
        if !session.autostart {
            continue;
        }
        match tmux::ensure_session(&session) {
            Ok(true) => println!("started {}", session.name),
            Ok(false) => println!("{} already running", session.name),
            Err(err) => {
                ok = false;
                eprintln!("failed to start {name}: {err:#}");
            }
        }
    }
    Ok(if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

fn autostop() -> Result<ExitCode> {
    let mut ok = true;
    for (name, path) in config::list_projects()? {
        let Ok(session) = config::load(&path) else {
            continue;
        };
        if !session.autostart || !tmux::has_session(&session.name) {
            continue;
        }
        match tmux::kill_session(&session.name) {
            Ok(()) => println!("stopped {}", session.name),
            Err(err) => {
                ok = false;
                eprintln!("failed to stop {name}: {err:#}");
            }
        }
    }
    Ok(if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

fn open_editor(path: &std::path::Path) -> Result<()> {
    let editor = env::var("VISUAL")
        .or_else(|_| env::var("EDITOR"))
        .ok()
        .filter(|e| !e.trim().is_empty())
        .unwrap_or_else(|| "vi".to_string());
    // $EDITOR may contain arguments (e.g. "code --wait"), so let the shell split it.
    let status = Command::new("sh")
        .arg("-c")
        .arg(format!("{editor} \"$1\""))
        .arg("sh")
        .arg(path)
        .status()
        .with_context(|| format!("failed to run editor '{editor}'"))?;
    if !status.success() {
        bail!("editor '{editor}' exited with {status}");
    }
    if let Err(err) = config::load(path) {
        eprintln!("warning: {err:#}");
    }
    Ok(())
}

fn confirm(prompt: &str) -> Result<bool> {
    print!("{prompt} [y/N] ");
    io::stdout().flush()?;
    let mut answer = String::new();
    io::stdin().read_line(&mut answer)?;
    Ok(matches!(answer.trim(), "y" | "Y" | "yes"))
}
