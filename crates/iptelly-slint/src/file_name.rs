// File names suggested in save dialogs. Ported from sanitizeFileName in
// src/app/utils.ts. Unlike the core's utils::sanitize, which builds the
// paths of downloads saved without asking, this keeps the name readable
// ("a/b" becomes "a_b", not "ab") and never returns an empty name.

use iptelly_core::utils;

pub fn sanitize(name: &str) -> String {
    let name: String = name
        .chars()
        .filter(|c| !c.is_ascii_control())
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c => c,
        })
        .collect();
    let name = name.trim_matches('.').trim();
    if name.is_empty() {
        "untitled".into()
    } else {
        name.into()
    }
}

/// The name followed by the extension of the stream at `url`.
pub fn for_stream(name: &str, url: &str) -> String {
    format!(
        "{}.{}",
        sanitize(name),
        utils::get_extension(url.to_string())
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replaces_characters_that_are_invalid_in_file_names() {
        assert_eq!(sanitize(r#"a/b\c:d*e?f"g<h>i|j"#), "a_b_c_d_e_f_g_h_i_j");
    }

    #[test]
    fn strips_control_characters() {
        assert_eq!(sanitize("a\x00b\x1Fc\x7F"), "abc");
    }

    #[test]
    fn strips_leading_and_trailing_dots() {
        assert_eq!(sanitize("..hidden.."), "hidden");
    }

    #[test]
    fn trims_surrounding_whitespace() {
        assert_eq!(sanitize("  My Channel  "), "My Channel");
    }

    #[test]
    fn keeps_inner_dots_and_spaces() {
        assert_eq!(sanitize("Show S01.E02 final"), "Show S01.E02 final");
    }

    #[test]
    fn falls_back_to_untitled_when_nothing_is_left() {
        assert_eq!(sanitize(""), "untitled");
        assert_eq!(sanitize("..."), "untitled");
        assert_eq!(sanitize("   "), "untitled");
    }

    #[test]
    fn for_stream_adds_the_stream_extension() {
        assert_eq!(
            for_stream("Film: Part 1", "http://example.com/movie/u/p/123.mkv"),
            "Film_ Part 1.mkv"
        );
        assert_eq!(
            for_stream("News", "http://example.com/get.php?username=a"),
            "News.mp4"
        );
    }
}
