use std::borrow::Cow;

/// Whether the local file system treats names that differ only in letter case as one file.
///
/// Windows (NTFS, ReFS, exFAT) does; Drive and Linux do not, so two cloud names can
/// collide once they are written beneath a local folder.
pub const fn local_names_ignore_case() -> bool {
    cfg!(windows)
}

/// Comparison key for a file name or path segment.
///
/// With `ignore_case` it applies the simple, one-to-one uppercase mapping used by NTFS,
/// never an expanding one (`ß` stays `ß`, it does not become `SS`), so names NTFS keeps
/// distinct are never merged. Without it the name is returned unchanged.
pub fn name_key(name: &str, ignore_case: bool) -> Cow<'_, str> {
    if !ignore_case {
        return Cow::Borrowed(name);
    }
    Cow::Owned(name.chars().map(simple_upper).collect())
}

/// Whether two names denote the same file on this platform's local file system.
pub fn same_local_name(a: &str, b: &str) -> bool {
    name_key(a, local_names_ignore_case()) == name_key(b, local_names_ignore_case())
}

fn simple_upper(c: char) -> char {
    let mut upper = c.to_uppercase();
    match (upper.next(), upper.next()) {
        (Some(single), None) => single,
        _ => c,
    }
}

/// Whether one path segment can be created as-is on Windows.
///
/// Rejects characters Windows forbids (including `:`, which would address an NTFS
/// alternate data stream), control characters, a trailing dot or space that Windows
/// silently strips, and the reserved device names with or without an extension.
pub fn is_windows_file_name(segment: &str) -> bool {
    if segment.is_empty()
        || segment.ends_with(['.', ' '])
        || segment
            .chars()
            .any(|c| c < ' ' || matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*'))
    {
        return false;
    }
    let stem = segment.split('.').next().unwrap_or(segment).trim_end();
    !is_reserved_device(stem)
}

fn is_reserved_device(stem: &str) -> bool {
    let upper = stem.to_ascii_uppercase();
    match upper.as_str() {
        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$" => true,
        _ => {
            let Some(number) = upper
                .strip_prefix("COM")
                .or_else(|| upper.strip_prefix("LPT"))
            else {
                return false;
            };
            // Windows also reserves the superscript digits for these devices.
            let mut digits = number.chars();
            matches!(
                (digits.next(), digits.next()),
                (Some('0'..='9' | '¹' | '²' | '³'), None)
            )
        }
    }
}
