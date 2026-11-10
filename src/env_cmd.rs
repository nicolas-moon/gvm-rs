use crate::config::Root;

/// Print the gvm-related environment of the current shell.
pub fn env(root: &Root) -> anyhow::Result<()> {
    println!("GVM_ROOT={}", root.path.display());
    for var in ["GOROOT", "GOPATH", "GVM_OVERLAY_PREFIX"] {
        match std::env::var(var) {
            Ok(v) => println!("{var}={v}"),
            Err(_) => println!("{var}= (not set)"),
        }
    }
    match root.active_version() {
        Some(v) => println!("active version: {v}"),
        None => println!("active version: (none)"),
    }
    if let Some(d) = root.default_name() {
        println!("default: {d}");
    }
    Ok(())
}
