# pfm - Port Forward Manager

A simple CLI tool for managing SSH port forwards with automatic port remapping and tracking.

## Features

- **Sensible defaults** - Just specify host and port
- **Automatic port remapping** - If port is occupied, finds next available
- **Persistent tracking** - Remembers all forwards across restarts; `pfm start all` brings them back
- **Beautiful colored output** - Clear status information
- **Process management** - Tracks and cleans up SSH processes
- **Shell completions** - For bash, zsh, fish

## Usage

### Add a port forward

```bash
pfm add user@server.com 8080:80
pfm add server.com 3000
```

Maps `localhost:8080` to `server.com:80`. When a single port is specified, it is used for both local and remote. If the local port is already in use, pfm automatically picks the next available port.

pfm runs ssh in the background with `BatchMode=yes`, so it cannot answer a password prompt. The host must accept key-based login (an SSH key or a running `ssh-agent`). If ssh fails to connect or cannot open the forward, `pfm add` shows the error ssh reported and saves nothing.

### List port forwards

```bash
pfm list
```

Lists all configured port forwards, including their assigned index, status (running or stopped), mapped ports, and PID.

### Stop port forwards

```bash
pfm stop 0
pfm stop 0 1 2
pfm stop all
```

Stops the SSH tunnel for specified forward indices or all forwards while keeping their configurations saved.

### Start port forwards

```bash
pfm start 0
pfm start 0 1 2
pfm start all
```

Relaunches stopped forwards on their saved ports. If a saved local port has become occupied, pfm automatically remaps it to an available port.

### Delete port forwards

```bash
pfm delete 0
pfm delete 0 1 2
pfm delete all
```

Stops running tunnels and deletes the forward entries from configuration.

### Clean up dead forwards

```bash
pfm cleanup
```

Removes forward entries whose background SSH processes have died unexpectedly.

### Shell completions

Generate completions for your shell:

**Bash**:
```bash
pfm completions bash > ~/.local/share/bash-completion/completions/pfm
# or add to ~/.bashrc:
eval "$(pfm completions bash)"
```

**Zsh**:
```bash
mkdir -p ~/.zfunc
pfm completions zsh > ~/.zfunc/_pfm
# ensure ~/.zfunc is in your $fpath in ~/.zshrc:
# fpath=(~/.zfunc $fpath)
# autoload -Uz compinit && compinit
```

**Fish**:
```bash
pfm completions fish > ~/.config/fish/completions/pfm.fish
```

## Installation

1. Cargo

```bash
cargo install --path .
```

2. NixOS

Add to flake.nix.

```nix
pfm.url = "github:bhaswata08/pfm";
```

Add pfm to `environment.systemPackages`

```nix
{
  inputs,
  ...
}: {
  environment.systemPackages = [
    inputs.pfm.packages.${pkgs.stdenv.hostPlatform.system}.default
  ];
}
```
