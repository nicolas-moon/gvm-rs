mod alias;
mod config;
mod diff;
mod env_cmd;
mod init;
mod install;
mod list;
mod pkgset;
mod releases;
mod use_version;
mod util;
mod version;

use std::path::PathBuf;

use clap::{Parser, Subcommand};

use crate::config::Root;

#[derive(Parser)]
#[command(
    name = "gvm-rs",
    version,
    about = "Go Version Manager — a Rust reimplementation of moovweb/gvm",
    long_about = "gvm-rs manages Go toolchain versions under $GVM_ROOT (~/.gvm by default). \
                  Commands that change the shell environment (`use`, `pkgset use/create`) print \
                  shell code on stdout for the sourced shim (`gvm`) to eval."
)]
struct Cli {
    /// Base directory for gvm data (default: $GVM_ROOT or ~/.gvm)
    #[arg(long, global = true, env = "GVM_ROOT")]
    root: Option<PathBuf>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Install a Go version (source build by default; --binary for a prebuilt tarball)
    Install {
        /// Version to install, e.g. "1.21.0", "1.21", "latest", "tip"
        version: String,
        /// Install from a custom source URL
        #[arg(short = 's', long)]
        source: Option<String>,
        /// Custom name for this version (instead of the tag)
        #[arg(short = 'n', long)]
        name: Option<String>,
        /// Install from a prebuilt binary only
        #[arg(short = 'B', long = "binary")]
        binary: bool,
        /// Try a prebuilt binary first, fall back to a source build
        #[arg(long = "prefer-binary")]
        prefer_binary: bool,
        /// Accepted for compatibility with gvm (no-op in gvm-rs)
        #[arg(short = 'b', long = "with-build-tools", hide = true)]
        with_build_tools: bool,
        /// Accepted for compatibility with gvm (no-op in gvm-rs)
        #[arg(short = 'p', long = "with-protobuf", hide = true)]
        with_protobuf: bool,
    },
    /// Uninstall a Go version
    Uninstall {
        /// Version name to remove
        name: String,
    },
    /// Activate a version in the current shell (prints env lines for the shim)
    Use {
        /// Version name to activate (default: the current default version)
        name: Option<String>,
        /// Also set this as the default version
        #[arg(long)]
        default: bool,
    },
    /// List installed Go versions
    List,
    /// List Go versions available for download (from go.dev)
    #[command(name = "listall")]
    ListAll {
        /// Include beta/RC versions
        #[arg(long)]
        all: bool,
    },
    /// Diff the active GOROOT against the clean cached copy (exit 1 on differences)
    Diff {
        /// Version to diff (default: the active version from $GOROOT)
        #[arg(long)]
        version: Option<String>,
    },
    /// Manage GOPATH package sets (like `gvm pkgset`)
    Pkgset {
        #[command(subcommand)]
        command: Option<PkgsetCommands>,
    },
    /// Manage version aliases
    Alias {
        #[command(subcommand)]
        command: Option<AliasCommands>,
    },
    /// Print the gvm environment of the current shell
    Env,
    /// Remove the gvm root directory and everything in it
    Implode {
        /// Skip the confirmation prompt
        #[arg(short = 'y', long)]
        yes: bool,
    },
    /// Install the shell shim and wire up the shell rc file
    Init {
        /// Shell to configure (auto-detected from $SHELL); bash or zsh
        #[arg(long)]
        shell: Option<String>,
        /// Explicit rc file to modify (default: ~/.bashrc or ~/.zshrc)
        #[arg(long)]
        rc: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum PkgsetCommands {
    /// Create a pkgset for the active version and switch to it
    Create {
        /// Pkgset name
        name: String,
    },
    /// Switch GOPATH to a pkgset of the active version
    Use {
        /// Pkgset name
        name: String,
    },
    /// Delete a pkgset of the active version
    Delete {
        /// Pkgset name
        name: String,
        /// Skip the confirmation prompt
        #[arg(short = 'y', long)]
        yes: bool,
    },
    /// Show the active pkgset name
    Current,
}

#[derive(Subcommand)]
enum AliasCommands {
    /// List aliases
    List,
    /// Create/update an alias: gvm-rs alias set <name> <target>
    Set { name: String, target: String },
    /// Delete an alias
    Delete { name: String },
}

fn main() {
    let cli = Cli::parse();
    let root = match cli.root {
        Some(p) => Root::new(p),
        None => Root::from_env(),
    };

    let result = match &cli.command {
        Commands::Install {
            version,
            source,
            name,
            binary,
            prefer_binary,
            ..
        } => {
            let opts = install::Opts {
                source_url: source.clone(),
                name: name.clone(),
                binary_only: *binary,
                prefer_binary: *prefer_binary,
            };
            install::install(&root, version, &opts).map(|()| 0)
        }
        Commands::Uninstall { name } => install::uninstall(&root, name).map(|()| 0),
        Commands::Use { name, default } => {
            use_version::use_version(&root, name.as_deref(), *default).map(|exports| {
                println!("{exports}");
                0
            })
        }
        Commands::List => list::list(&root).map(|()| 0),
        Commands::ListAll { all } => list::listall(*all).map(|()| 0),
        Commands::Diff { version } => diff::diff(&root, version.clone()),
        Commands::Pkgset { command } => match command {
            Some(PkgsetCommands::Create { name }) => pkgset::create(&root, name).map(|exports| {
                println!("{exports}");
                0
            }),
            Some(PkgsetCommands::Use { name }) => pkgset::use_pkgset(&root, name).map(|exports| {
                println!("{exports}");
                0
            }),
            Some(PkgsetCommands::Delete { name, yes }) => {
                pkgset::delete(&root, name, *yes).map(|()| 0)
            }
            Some(PkgsetCommands::Current) => pkgset::current(&root).map(|name| {
                println!("{name}");
                0
            }),
            None => pkgset::list(&root).map(|()| 0),
        },
        Commands::Alias { command } => match command {
            Some(AliasCommands::List) => alias::list(&root).map(|()| 0),
            Some(AliasCommands::Set { name, target }) => {
                alias::set(&root, name, target).map(|()| 0)
            }
            Some(AliasCommands::Delete { name }) => alias::delete(&root, name).map(|()| 0),
            None => alias::list(&root).map(|()| 0),
        },
        Commands::Env => env_cmd::env(&root).map(|()| 0),
        Commands::Implode { yes } => install::implode(&root, *yes).map(|()| 0),
        Commands::Init { shell, rc } => {
            init::init(&root, shell.as_deref(), rc.as_deref()).map(|()| 0)
        }
    };

    match result {
        Ok(code) => std::process::exit(code),
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    }
}
