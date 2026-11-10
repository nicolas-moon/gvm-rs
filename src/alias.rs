use crate::config::{validate_name, Root};

pub fn list(root: &Root) -> anyhow::Result<()> {
    let aliases = root.aliases();
    if aliases.is_empty() {
        println!("No aliases.");
        return Ok(());
    }
    for (name, target) in &aliases {
        let resolved = root
            .version_of_name(target)
            .map(|v| v.tag())
            .unwrap_or_else(|| target.clone());
        println!("{name} -> {target} ({resolved})");
    }
    Ok(())
}

pub fn set(root: &Root, name: &str, target: &str) -> anyhow::Result<()> {
    validate_name(name)?;
    validate_name(target)?;
    root.set_alias(name, target)?;
    println!("Set alias {name} -> {target}");
    Ok(())
}

pub fn delete(root: &Root, name: &str) -> anyhow::Result<()> {
    validate_name(name)?;
    if root.remove_alias(name)? {
        println!("Deleted alias {name}");
    } else {
        anyhow::bail!("no such alias: {name}");
    }
    Ok(())
}
