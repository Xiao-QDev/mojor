#[derive(Eq, PartialEq, Debug)]
pub enum CommandVersion {
    Version,
}

impl CommandVersion {
    pub fn message(&self) -> &'static str {
        "This server is running Mojor version 0.0.1 (MCPE: 0.14.3) (Plugin API 0.0.1)"
    }
}