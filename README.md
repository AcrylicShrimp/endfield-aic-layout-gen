# endfield-aic-layout-gen

CLI tools for generating construction layouts for the Arknights: Endfield Automated Industry Complex.

## Workspace

This repository is a Rust virtual workspace. Crates live under `crates/` and use the `aic-` prefix.

- `crates/aic-cli`: command line entry point

Facility and recipe data must stay outside the compiled application and be loaded from external data files at runtime.
