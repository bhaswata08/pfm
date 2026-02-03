# pfm - Port Forward Manager

A simple CLI tool for managing SSH port forwards with automatic port remapping and tracking.

## Features

- **Sensible defaults** - Just specify host and port
- **Automatic port remapping** - If port is occupied, finds next available
- **Persistent tracking** - Remembers all forwards across restarts
- **Beautiful colored output** - Clear status information
- **Process management** - Tracks and cleans up SSH processes
- **Shell completions** - For bash, zsh, fish

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

Add pfm to `envrionement.systemPackages`

```nix
{
  inputs,
  ...
}: {
  environment.systemPackages = [
    inputs.nix.packages.${pkgs.stdenv.hostPlatform.system}.default
  ];
}
```
