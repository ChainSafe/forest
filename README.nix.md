# Using Nix with Forest

This guide will help you get started with building and installing Forest using
Nix.

## Installing Nix

The recommended way to install Nix is using Determinate Systems' Nix installer:

1. Run the following command in your terminal:
   ```bash
   curl -fsSL https://install.determinate.systems/nix | sh -s -- install --determinate
   ```

2. After installation, restart your shell or run:
   ```bash
   . /nix/var/nix/profiles/default/etc/profile.d/nix-daemon.sh
   ```

See [Determinate Systems' Nix installation
guide](https://docs.determinate.systems/getting-started/individuals) for more
information.

## Building Forest with Flakes

Forest uses Nix flakes for reproducible builds. To build Forest:

1. Clone the Forest repository
2. Run the following command in the repository root:
   ```bash
   nix build
   ```

This will build Forest and all its dependencies in a reproducible environment.
Note, you might see a warning from FlakeHub. This happens when you're not logged
in (to FlakeHub) and can be ignored.

## Installing Forest

To install Forest directly from the repository:

```bash
nix profile install .
```

This will make the following Forest commands available in your shell:
- `forest` - The main Forest daemon
- `forest-cli` - Command line interface for interacting with Forest
- `forest-tool` - Utility tools for Forest
- `forest-wallet` - Forest wallet management

You can now run any of these commands directly from your shell.

## Upgrading Forest

To upgrade Forest to the latest version:

```bash
nix profile upgrade forest
```

## Removing Forest

To remove Forest from your system:

```bash
nix profile remove forest
```

You can also list your current profile installations using:

```bash
nix profile list
```

## Troubleshooting

If you encounter any issues with Nix:
- Make sure you have the latest version of Nix installed
- Try running `nix-collect-garbage` if you're running low on disk space

### Build Environment Limitations

- Nix builds occur in a read-only environment. This means that build scripts
  cannot generate or modify source files during the build process
- If your build requires non-Rust files (e.g., JSON, proto files, or other
  assets), they must be explicitly listed in the flake.nix file
- If you see errors about missing files during the build, check that they are
  properly included in the flake's source inputs

Example error messages you might encounter:
- "Permission denied" when trying to write files during build
- "File not found" for non-Rust files that aren't explicitly included in the
  flake
