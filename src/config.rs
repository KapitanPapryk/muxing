use std::{
    env, fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use serde::Deserialize;

/// A single project file (`<config dir>/<name>.yml`).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Project {
    /// tmux session name; defaults to the file stem.
    pub name: Option<String>,
    /// Working directory for every window/pane unless overridden.
    pub root: Option<String>,
    /// Start this project from `muxing autostart` (the systemd service).
    #[serde(default)]
    pub autostart: bool,
    #[serde(default)]
    pub windows: Vec<Window>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Window {
    pub name: Option<String>,
    pub root: Option<String>,
    /// tmux layout name (`tiled`, `main-vertical`, ...) or a layout string.
    pub layout: Option<String>,
    /// `null` entries (a bare `-`) are empty panes.
    #[serde(default)]
    pub panes: Vec<Option<Pane>>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum Pane {
    Command(String),
    Commands(Vec<String>),
    Detailed(PaneDetails),
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PaneDetails {
    pub root: Option<String>,
    #[serde(default)]
    pub commands: Vec<String>,
}

/// Fully resolved session description, ready to be handed to tmux.
#[derive(Debug, PartialEq)]
pub struct Session {
    pub name: String,
    pub autostart: bool,
    pub windows: Vec<ResolvedWindow>,
}

#[derive(Debug, PartialEq)]
pub struct ResolvedWindow {
    pub name: Option<String>,
    pub layout: Option<String>,
    pub panes: Vec<ResolvedPane>,
}

#[derive(Debug, PartialEq)]
pub struct ResolvedPane {
    pub root: PathBuf,
    pub commands: Vec<String>,
}

/// Directory holding project files.
pub fn config_dir() -> Result<PathBuf> {
    if let Some(dir) = env::var_os("MUXING_CONFIG_DIR").filter(|v| !v.is_empty()) {
        return Ok(PathBuf::from(dir));
    }
    if let Some(dir) = env::var_os("XDG_CONFIG_HOME").filter(|v| !v.is_empty()) {
        return Ok(PathBuf::from(dir).join("muxing"));
    }
    Ok(home_dir()?.join(".config").join("muxing"))
}

pub fn home_dir() -> Result<PathBuf> {
    env::var_os("HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .context("$HOME is not set")
}

/// Path of an existing project file, trying `.yml` then `.yaml`.
pub fn find_project(name: &str) -> Result<PathBuf> {
    let dir = config_dir()?;
    for ext in ["yml", "yaml"] {
        let path = dir.join(format!("{name}.{ext}"));
        if path.is_file() {
            return Ok(path);
        }
    }
    bail!("no project '{name}' in {}", dir.display())
}

/// All project files in the config dir, sorted by name.
pub fn list_projects() -> Result<Vec<(String, PathBuf)>> {
    let dir = config_dir()?;
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for entry in fs::read_dir(&dir).with_context(|| format!("reading {}", dir.display()))? {
        let path = entry?.path();
        let is_yaml = matches!(
            path.extension().and_then(|e| e.to_str()),
            Some("yml" | "yaml")
        );
        if is_yaml
            && path.is_file()
            && let Some(stem) = path.file_stem().and_then(|s| s.to_str())
        {
            out.push((stem.to_string(), path.clone()));
        }
    }
    out.sort();
    Ok(out)
}

pub fn load(path: &Path) -> Result<Session> {
    let text = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("muxing");
    parse(&text, stem).with_context(|| format!("in {}", path.display()))
}

pub fn parse(text: &str, default_name: &str) -> Result<Session> {
    let project: Project = serde_yaml::from_str(text)?;
    project.resolve(default_name)
}

impl Project {
    pub fn resolve(self, default_name: &str) -> Result<Session> {
        let name = sanitize_name(self.name.as_deref().unwrap_or(default_name));
        if name.is_empty() {
            bail!("session name is empty");
        }
        let home = home_dir()?;
        let project_root = match &self.root {
            Some(r) => resolve_path(r, &home)?,
            None => home,
        };

        let mut windows = Vec::new();
        for window in self.windows {
            let window_root = match &window.root {
                Some(r) => resolve_path(r, &project_root)?,
                None => project_root.clone(),
            };
            let mut panes = Vec::new();
            for pane in window.panes {
                panes.push(match pane {
                    None => ResolvedPane {
                        root: window_root.clone(),
                        commands: vec![],
                    },
                    Some(Pane::Command(cmd)) => ResolvedPane {
                        root: window_root.clone(),
                        commands: vec![cmd],
                    },
                    Some(Pane::Commands(commands)) => ResolvedPane {
                        root: window_root.clone(),
                        commands,
                    },
                    Some(Pane::Detailed(d)) => ResolvedPane {
                        root: match &d.root {
                            Some(r) => resolve_path(r, &window_root)?,
                            None => window_root.clone(),
                        },
                        commands: d.commands,
                    },
                });
            }
            if panes.is_empty() {
                panes.push(ResolvedPane {
                    root: window_root,
                    commands: vec![],
                });
            }
            windows.push(ResolvedWindow {
                name: window.name,
                layout: window.layout,
                panes,
            });
        }
        if windows.is_empty() {
            windows.push(ResolvedWindow {
                name: None,
                layout: None,
                panes: vec![ResolvedPane {
                    root: project_root,
                    commands: vec![],
                }],
            });
        }

        Ok(Session {
            name,
            autostart: self.autostart,
            windows,
        })
    }
}

/// tmux treats `.` and `:` as target separators, so they can't appear in session names.
pub fn sanitize_name(name: &str) -> String {
    name.trim()
        .chars()
        .map(|c| if c == '.' || c == ':' { '_' } else { c })
        .collect()
}

/// Expand `~` and `$VARS`; relative paths are joined onto `base`.
fn resolve_path(raw: &str, base: &Path) -> Result<PathBuf> {
    let expanded = shellexpand::full(raw).with_context(|| format!("expanding path '{raw}'"))?;
    let path = PathBuf::from(expanded.as_ref());
    Ok(if path.is_absolute() {
        path
    } else {
        base.join(path)
    })
}

pub const TEMPLATE: &str = r#"# muxing project
# name: defaults to the file name
root: ~/
# start this session from the systemd service (muxing service install)
autostart: false

windows:
  - name: editor
    layout: main-vertical
    panes:
      - echo "single command"
      - [echo "first", echo "second"]
      - root: /tmp
        commands:
          - ls
  - name: shell
"#;

#[cfg(test)]
mod tests {
    use super::*;

    fn home() -> PathBuf {
        home_dir().unwrap()
    }

    #[test]
    fn parses_all_pane_forms() {
        let s = parse(
            r#"
name: web.app
root: /srv/app
autostart: true
windows:
  - name: editor
    root: src
    layout: main-vertical
    panes:
      - nvim
      - [cd api, cargo run]
      - root: /tmp
        commands: [ls]
      - root: web
      -
  - name: shell
"#,
            "ignored",
        )
        .unwrap();
        assert_eq!(s.name, "web_app");
        assert!(s.autostart);
        assert_eq!(s.windows.len(), 2);
        let w = &s.windows[0];
        assert_eq!(w.layout.as_deref(), Some("main-vertical"));
        let src = PathBuf::from("/srv/app/src");
        assert_eq!(
            w.panes,
            vec![
                ResolvedPane {
                    root: src.clone(),
                    commands: vec!["nvim".into()]
                },
                ResolvedPane {
                    root: src.clone(),
                    commands: vec!["cd api".into(), "cargo run".into()]
                },
                ResolvedPane {
                    root: "/tmp".into(),
                    commands: vec!["ls".into()]
                },
                ResolvedPane {
                    root: src.join("web"),
                    commands: vec![]
                },
                ResolvedPane {
                    root: src.clone(),
                    commands: vec![]
                },
            ]
        );
        assert_eq!(
            s.windows[1].panes,
            vec![ResolvedPane {
                root: "/srv/app".into(),
                commands: vec![]
            }]
        );
    }

    #[test]
    fn defaults() {
        let s = parse("{}", "proj").unwrap();
        assert_eq!(s.name, "proj");
        assert!(!s.autostart);
        assert_eq!(s.windows.len(), 1);
        assert_eq!(s.windows[0].panes[0].root, home());
    }

    #[test]
    fn tilde_expansion() {
        let s = parse("root: ~/code", "p").unwrap();
        assert_eq!(s.windows[0].panes[0].root, home().join("code"));
    }

    #[test]
    fn unknown_field_is_error() {
        assert!(parse("autostrat: true", "p").is_err());
        assert!(parse("windows: [{name: a, pane: [x]}]", "p").is_err());
    }

    #[test]
    fn template_and_example_parse() {
        parse(TEMPLATE, "t").unwrap();
        parse(include_str!("../examples/example.yml"), "example").unwrap();
    }
}
