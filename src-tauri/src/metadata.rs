use std::path::Path;

use lofty::file::AudioFile;
use lofty::file::TaggedFileExt;
use lofty::picture::MimeType;
use lofty::tag::{Accessor, ItemKey};

use crate::types::{AppError, Track};

/// Extract metadata from an audio file at `path`.
///
/// `cache_dir` is the application cache directory. When provided and the file
/// contains embedded cover art, the image is written to
/// `{cache_dir}/covers/<hex_hash>.<ext>` and the path is stored in the
/// returned track's `cover_art_path` field.
///
/// Returns a partially-populated `Track` (id = 0, missing = false).
/// Fields absent from the file are set to `None`.
/// Only returns an error for actual IO / parse failures.
pub fn extract_metadata(path: &Path, cache_dir: Option<&Path>) -> Result<Track, AppError> {
    let path_str = path
        .to_str()
        .ok_or_else(|| AppError::Io(format!("Non-UTF-8 path: {:?}", path)))?
        .to_string();

    let dir_path = path
        .parent()
        .and_then(|p| p.to_str())
        .unwrap_or("")
        .to_string();

    let filename = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_string();

    // Open and parse the file with lofty.
    let tagged_file = lofty::read_from_path(path)
        .map_err(|e| AppError::Decode(format!("Failed to read tags from {:?}: {}", path, e)))?;

    // Duration from audio properties.
    let duration_secs: Option<f64> = {
        let d = tagged_file.properties().duration();
        let secs = d.as_secs_f64();
        if secs > 0.0 { Some(secs) } else { None }
    };

    // Use the primary tag, falling back to the first available tag.
    let tag = tagged_file.primary_tag().or_else(|| tagged_file.first_tag());

    let (title, artist, album, album_artist, year, genre, track_number, disc_number, cover_art_path) =
        if let Some(tag) = tag {
            let title = tag.title().map(|s| s.to_string());
            let artist = tag.artist().map(|s| s.to_string());
            let album = tag.album().map(|s| s.to_string());
            let genre = tag.genre().map(|s| s.to_string());

            let album_artist = tag
                .get_string(&ItemKey::AlbumArtist)
                .map(|s| s.to_string());

            let year: Option<i32> = tag
                .get_string(&ItemKey::Year)
                .and_then(|s| s.parse::<i32>().ok())
                .or_else(|| {
                    // Some formats store year under RecordingDate
                    tag.get_string(&ItemKey::RecordingDate)
                        .and_then(|s| s.get(..4))
                        .and_then(|s| s.parse::<i32>().ok())
                });

            let track_number: Option<u32> = tag
                .get_string(&ItemKey::TrackNumber)
                .and_then(|s| {
                    // Track numbers may be "3/12" — take the part before '/'
                    s.split('/').next().and_then(|n| n.trim().parse::<u32>().ok())
                });

            let disc_number: Option<u32> = tag
                .get_string(&ItemKey::DiscNumber)
                .and_then(|s| {
                    s.split('/').next().and_then(|n| n.trim().parse::<u32>().ok())
                });

            // Cover art extraction.
            let cover_art_path = extract_cover_art(tag.pictures(), cache_dir, &path_str);

            (title, artist, album, album_artist, year, genre, track_number, disc_number, cover_art_path)
        } else {
            (None, None, None, None, None, None, None, None, None)
        };

    Ok(Track {
        id: 0,
        path: path_str,
        dir_path,
        filename,
        title,
        artist,
        album,
        album_artist,
        year,
        genre,
        track_number,
        disc_number,
        duration_secs,
        cover_art_path,
        missing: false,
        bpm: None,
    })
}

/// Write the first available picture to the covers cache directory.
/// Returns the path as a `String`, or `None` if there are no pictures or
/// writing fails.
fn extract_cover_art(
    pictures: &[lofty::picture::Picture],
    cache_dir: Option<&Path>,
    track_path: &str,
) -> Option<String> {
    let cache_dir = cache_dir?;
    let picture = pictures.first()?;

    let ext = match picture.mime_type() {
        Some(MimeType::Png) => "png",
        Some(MimeType::Jpeg) => "jpg",
        Some(MimeType::Gif) => "gif",
        Some(MimeType::Bmp) => "bmp",
        Some(MimeType::Tiff) => "tiff",
        _ => "jpg", // default to jpg for unknown types
    };

    // Use a simple hash of the track path as the filename to avoid collisions.
    let hash = simple_hash(track_path);
    let covers_dir = cache_dir.join("covers");

    if std::fs::create_dir_all(&covers_dir).is_err() {
        return None;
    }

    let file_name = format!("{:016x}.{}", hash, ext);
    let dest = covers_dir.join(&file_name);

    // Skip writing if the file already exists (idempotent).
    if dest.exists() {
        return dest.to_str().map(|s| s.to_string());
    }

    std::fs::write(&dest, picture.data()).ok()?;
    dest.to_str().map(|s| s.to_string())
}

/// A fast, non-cryptographic hash of a string (FNV-1a 64-bit).
fn simple_hash(s: &str) -> u64 {
    let mut hash: u64 = 14695981039346656037;
    for byte in s.bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(1099511628211);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_hash_deterministic() {
        let h1 = simple_hash("/music/test.mp3");
        let h2 = simple_hash("/music/test.mp3");
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_simple_hash_different_inputs() {
        let h1 = simple_hash("/music/a.mp3");
        let h2 = simple_hash("/music/b.mp3");
        assert_ne!(h1, h2);
    }

    #[test]
    fn test_extract_metadata_nonexistent_file_returns_error() {
        let result = extract_metadata(Path::new("/nonexistent/file.mp3"), None);
        assert!(result.is_err());
    }

    #[test]
    fn test_extract_metadata_path_fields() {
        // We can't easily test with a real audio file in unit tests,
        // but we verify the error path for a non-existent file.
        let path = Path::new("/tmp/does_not_exist.flac");
        let result = extract_metadata(path, None);
        assert!(result.is_err());
    }
}
