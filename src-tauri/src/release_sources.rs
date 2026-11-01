//! Only mirror the same pinned artifact between the two publisher repositories.
const BASES: [&str; 2] = [
    "https://gitee.com/wu-dinglin/shannon/releases/download/",
    "https://github.com/DinglinWu/Shannon/releases/download/",
];

pub fn official_url(url: &str) -> bool {
    BASES.iter().any(|base| url.starts_with(base))
}

pub fn download_urls(url: &str) -> Vec<String> {
    let mut urls = vec![url.to_owned()];
    if let Some((index, suffix)) = BASES.iter().enumerate()
        .find_map(|(i, base)| url.strip_prefix(base).map(|s| (i, s))) {
        urls.push(format!("{}{}", BASES[1 - index], suffix));
    }
    urls
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mirrors_preserve_version_and_filename_in_both_directions() {
        for i in 0..2 {
            let url = format!("{}v3.0.2/file.zip", BASES[i]);
            assert_eq!(download_urls(&url), vec![url, format!("{}v3.0.2/file.zip", BASES[1-i])]);
        }
    }
    #[test]
    fn other_hosts_and_repository_names_are_not_mirrored() {
        for url in ["https://github.com/other/Shannon/releases/download/v1/a.exe",
            "http://gitee.com/wu-dinglin/shannon/releases/download/v1/a.exe",
            "https://github.com.evil/DinglinWu/Shannon/releases/download/v1/a.exe"] {
            assert!(!official_url(url));
            assert_eq!(download_urls(url), vec![url.to_owned()]);
        }
    }
}
