use std::path::PathBuf;

/// NFD normalization stub for macOS filenames
/// TODO M1: integrate with unicode-normalization crate
#[must_use]
pub fn nfd_normalize(s: &str) -> String {
    // Placeholder: identity function for M0
    s.to_owned()
}

/// Get the path to the application database
#[must_use]
pub fn app_db_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(home).join(".filesync").join("filesync.db")
}

/// Get the path to the identity file (certificate + private key)
#[must_use]
pub fn identity_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(home).join(".filesync").join("identity.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nfd_normalize_roundtrips_ascii() {
        let input = "hello_world.txt";
        assert_eq!(nfd_normalize(input), input);
    }
}
