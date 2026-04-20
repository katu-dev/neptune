use std::path::MAIN_SEPARATOR;

/// Converts any path string to use the OS-native path separator.
/// Handles empty strings and paths with mixed separators (`/` and `\`).
pub fn normalize_path(path: &str) -> String {
    if path.is_empty() {
        return String::new();
    }
    path.chars()
        .map(|c| if c == '/' || c == '\\' { MAIN_SEPARATOR } else { c })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::MAIN_SEPARATOR;

    #[test]
    fn test_empty_string() {
        assert_eq!(normalize_path(""), "");
    }

    #[test]
    fn test_forward_slashes() {
        let result = normalize_path("music/rock/song.mp3");
        let expected = format!("music{0}rock{0}song.mp3", MAIN_SEPARATOR);
        assert_eq!(result, expected);
    }

    #[test]
    fn test_backslashes() {
        let result = normalize_path("music\\rock\\song.mp3");
        let expected = format!("music{0}rock{0}song.mp3", MAIN_SEPARATOR);
        assert_eq!(result, expected);
    }

    #[test]
    fn test_mixed_separators() {
        let result = normalize_path("music/rock\\song.mp3");
        let expected = format!("music{0}rock{0}song.mp3", MAIN_SEPARATOR);
        assert_eq!(result, expected);
    }

    #[test]
    fn test_no_separators() {
        assert_eq!(normalize_path("song.mp3"), "song.mp3");
    }
}
