use std::fs;
use std::path::Path;

use crate::config::Root;

/// Embedded copy of the shell shim, installed at `$GVM_ROOT/scripts/gvm`.
pub const SHIM: &str = include_str!("../shim/gvm.sh");

const RC_BEGIN: &str = "# >>> gvm-rs >>>";
const RC_END: &str = "# <<< gvm-rs <<<";

/// Install the shim and wire the current shell's rc file to source it.
pub fn init(root: &Root, shell: Option<&str>, rc: Option<&Path>) -> anyhow::Result<()> {
    root.ensure_dirs()?;
    let shim_path = root.scripts_dir().join("gvm");
    fs::write(&shim_path, SHIM)?;
    println!("Wrote {}", shim_path.display());

    let rc_path = match rc {
        Some(p) => p.to_path_buf(),
        None => default_rc_file(shell)?,
    };

    let existing = fs::read_to_string(&rc_path).unwrap_or_default();
    if existing.contains(RC_BEGIN) {
        println!(
            "{} already contains the gvm-rs block; leaving it alone.",
            rc_path.display()
        );
    } else {
        let mut block = String::new();
        if !existing.is_empty() && !existing.ends_with('\n') {
            block.push('\n');
        }
        block.push('\n');
        block.push_str(RC_BEGIN);
        block.push('\n');
        block.push_str(&format!(
            "[ -s \"{}\" ] && source \"{}\"\n",
            shim_path.display(),
            shim_path.display()
        ));
        block.push_str(RC_END);
        let mut out = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&rc_path)?;
        use std::io::Write;
        out.write_all(block.as_bytes())?;
        println!("Added the gvm-rs source line to {}", rc_path.display());
    }

    println!(
        "\nDone. Open a new shell (or run: source {}), then:",
        rc_path.display()
    );
    println!("  gvm install go1.21 --binary");
    println!("  gvm use go1.21 --default");
    Ok(())
}

fn default_rc_file(shell: Option<&str>) -> anyhow::Result<std::path::PathBuf> {
    let name = shell
        .map(|s| s.to_string())
        .or_else(|| std::env::var("SHELL").ok())
        .unwrap_or_default();
    let base = name.rsplit('/').next().unwrap_or(&name);
    let rc = match base {
        "zsh" => "zshrc",
        "bash" => "bashrc",
        "fish" => {
            anyhow::bail!(
                "fish is not supported yet (the shim relies on eval); use bash or zsh, or pass --rc"
            )
        }
        _ => "bashrc",
    };
    Ok(crate::util::home_dir().join(format!(".{rc}")))
}
