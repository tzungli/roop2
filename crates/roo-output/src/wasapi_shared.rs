use anyhow::Result;

pub struct SharedOutputController {
    // Placeholder for WASAPI shared mode
}

impl Default for SharedOutputController {
    fn default() -> Self {
        Self::new()
    }
}

impl SharedOutputController {
    pub fn new() -> Self {
        Self {}
    }

    pub fn start(&self) -> Result<()> {
        Ok(())
    }
}
