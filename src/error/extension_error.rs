
#[derive(Debug, Clone)]
pub struct ExtensionError;

impl std::fmt::Display for ExtensionError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "Extension error")
    }
}

impl std::error::Error for ExtensionError {}
