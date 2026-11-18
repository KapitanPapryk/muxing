# muxing

A small [tmuxinator](https://github.com/tmuxinator/tmuxinator)-like tmux session manager written in Rust.
Projects are YAML files that describe windows, panes and startup commands. Projects marked
`autostart: true` are started by a systemd user service at boot or login.

## Install

```sh
cargo install --path .
```

## Config directory

One file per project, `<name>.yml` (or `.yaml`), in the first of these that is set:

1. `$MUXING_CONFIG_DIR`
2. `$XDG_CONFIG_HOME/muxing`
3. `~/.config/muxing`

`muxing dir` prints the directory in use.

## Project file

```yaml
name: webapp            # tmux session name (default: file name; '.' and ':' become '_')
root: ~/code/webapp     # working dir for everything below (default: $HOME); ~ and $VARS expand
autostart: true         # start from the systemd service (default: false)

windows:
  - name: editor
    root: src                    # relative paths resolve against the parent root
    layout: main-vertical        # any tmux layout (default: tiled if >1 pane)
    panes:
      - nvim                     # one command
      - [cd api, cargo watch -x run]   # several commands, run in order
      - root: ~/code/webapp/web  # pane-specific working dir
        commands:
          - npm run dev
      -                          # empty pane
  - name: shell                  # no panes -> one empty shell
```

Unknown keys are rejected, so typos show up in `muxing check`.
Quote commands that contain `: ` (e.g. `- "echo a: b"`), or YAML reads them as a map.
See [`examples/example.yml`](examples/example.yml).

## Commands

| command | |
|---|---|
| `muxing start <name> [-d]` | create the session if needed and attach (`-d`: don't attach). Inside tmux it switches client. |
| `muxing stop <name>` | kill the session |
| `muxing list` | projects, autostart flag, running status |
| `muxing new <name>` | create a project from a template and open `$EDITOR` |
| `muxing edit <name>` | open a project in `$EDITOR` |
| `muxing delete <name> [-y]` | delete a project file |
| `muxing check [name]` | validate one or all projects |
| `muxing autostart` | start (detached) every project with `autostart: true` |
| `muxing autostop` | stop every running project with `autostart: true` |
| `muxing service install [--now]` | write and enable the systemd user service |
| `muxing service uninstall` | stop, disable and remove it |
| `muxing service status` | service status and linger state |

Set `MUXING_TMUX` to use a different tmux binary or wrapper.

## systemd service

```sh
muxing service install --now
loginctl enable-linger $USER   # start at boot, without logging in
```

This writes `~/.config/systemd/user/muxing.service` pointing at the installed binary. The
service runs `muxing autostart` on start and `muxing autostop` on stop
(`systemctl --user stop muxing` kills the autostarted sessions). Without lingering the
service starts at your first login instead of at boot.
A reference unit is in [`systemd/muxing.service`](systemd/muxing.service).

Sessions started by the service get the systemd user environment, so variables from your
desktop session (e.g. `DISPLAY`, `WAYLAND_DISPLAY`) won't be set in those panes.
