/// NFD normalization stub for macOS filenames
/// TODO M1: integrate with unicode-normalization crate
#[must_use]
pub fn nfd_normalize(s: &str) -> String {
    // Placeholder: identity function for M0
    s.to_owned()
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
