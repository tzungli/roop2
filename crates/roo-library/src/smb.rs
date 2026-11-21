use std::fs;
use std::path::Path;
use anyhow::{bail, Result};

pub fn test_samba_share(unc_path: &str) -> Result<bool> {
    let p = Path::new(unc_path);
    if !p.exists() {
        bail!("Path does not exist: {}", unc_path);
    }
    match fs::read_dir(p) {
        Ok(_) => Ok(true),
        Err(e) => bail!("Failed to read network directory: {}", e),
    }
}
