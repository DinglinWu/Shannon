use serde::Deserialize;
#[derive(Deserialize)]
struct Avatar {
    id: String,
    keywords: Vec<String>,
}
fn catalog() -> &'static Vec<Avatar> {
    static ITEMS: std::sync::OnceLock<Vec<Avatar>> = std::sync::OnceLock::new();
    ITEMS.get_or_init(|| {
        serde_json::from_str(include_str!("../../src/wechat-avatar-catalog.json"))
            .expect("bundled avatar catalog")
    })
}
pub fn valid(id: &str) -> bool {
    id.is_empty() || custom(id) || catalog().iter().any(|a| a.id == id)
}
pub fn assign(name: &str, card: &crate::wechat_story::Card) -> String {
    if !card.avatar.is_empty() && valid(&card.avatar) {
        return card.avatar.clone();
    }
    let description = format!(
        "{name} {} {} {} {} {}",
        card.personality, card.current_state, card.history, card.world, card.opening
    );
    let best = catalog()
        .iter()
        .map(|a| {
            (
                a,
                a.keywords
                    .iter()
                    .filter(|k| description.contains(k.as_str()))
                    .map(|k| k.chars().count())
                    .sum::<usize>(),
            )
        })
        .max_by_key(|(_, score)| *score);
    if let Some((avatar, score)) = best {
        if score > 0 {
            return avatar.id.clone();
        }
    }
    let hash = name.as_bytes().iter().fold(2166136261u32, |hash, b| {
        (hash ^ (*b as u32)).wrapping_mul(16777619)
    });
    catalog()[hash as usize % catalog().len()].id.clone()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn uploaded_images_are_bounded_decoded_and_ids_cannot_escape_avatar_directory() {
        let id = format!("custom-{}", "a".repeat(64));
        assert!(valid(&id));
        assert!(custom(&id));
        assert!(!custom("custom-../../secret"));
        assert!(!custom(&format!("custom-{}", "A".repeat(64))));
        let card = crate::wechat_story::Card {
            avatar: id.clone(),
            ..Default::default()
        };
        assert_eq!(assign("小林", &card), id);
        fn png_bytes(size: u32) -> Vec<u8> {
            let mut bytes = vec![];
            {
                let mut encoder = png::Encoder::new(&mut bytes, size, size);
                encoder.set_color(png::ColorType::Rgba);
                encoder.set_depth(png::BitDepth::Eight);
                let mut writer = encoder.write_header().unwrap();
                writer
                    .write_image_data(&vec![255; (size * size * 4) as usize])
                    .unwrap();
            }
            bytes
        }
        assert!(clean_png(&png_bytes(256)).is_ok());
        assert!(clean_png(&png_bytes(32)).is_err());
        assert!(clean_png(b"<svg onload='alert(1)'/>").is_err());
        assert!(clean_png(&vec![0; 300001]).is_err());
        let mut truncated = png_bytes(256);
        truncated.truncate(40);
        assert!(clean_png(&truncated).is_err());
    }
    #[test]
    fn assignment_matches_explicit_traits_and_preserves_manual_choices() {
        let mut card = crate::wechat_story::Card::default();
        card.personality = "戴眼镜的研究员".into();
        assert_eq!(assign("小林", &card), "glasses");
        card.avatar = "sea".into();
        assert_eq!(assign("小林", &card), "sea");
        assert!(!valid("../../file"));
        assert!(!valid("https://example.com/a.png"));
        assert_eq!(
            assign("新朋友", &Default::default()),
            assign("新朋友", &Default::default())
        );
        assert!(valid(&assign("新朋友", &Default::default())));
    }
}

pub fn custom(id: &str) -> bool {
    id.strip_prefix("custom-").is_some_and(|hash| {
        hash.len() == 64
            && hash
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}
fn clean_png(bytes: &[u8]) -> Result<Vec<u8>, String> {
    if bytes.len() > 300_000 {
        return Err("头像文件过大".into());
    }
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_limits(png::Limits { bytes: 1_048_576 });
    let mut reader = decoder.read_info().map_err(|_| "头像图片无效")?;
    let info = reader.info();
    if info.width != 256
        || info.height != 256
        || info.bit_depth != png::BitDepth::Eight
        || !matches!(info.color_type, png::ColorType::Rgba | png::ColorType::Rgb)
        || info.animation_control.is_some()
    {
        return Err("头像须为256×256的静态PNG图片".into());
    }
    let mut buffer = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buffer).map_err(|_| "头像图片损坏")?;
    let mut clean = vec![];
    {
        let mut encoder = png::Encoder::new(&mut clean, 256, 256);
        encoder.set_color(info.color_type);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
        writer
            .write_image_data(&buffer[..info.buffer_size()])
            .map_err(|e| e.to_string())?;
    }
    Ok(clean)
}
pub fn import_image(app: &tauri::AppHandle, data: &str) -> Result<String, String> {
    use base64::Engine;
    use sha2::{Digest, Sha256};
    if data.len() > 400_000 {
        return Err("头像文件过大".into());
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data)
        .map_err(|_| "头像编码无效")?;
    let clean = clean_png(&bytes)?;
    let id = format!("custom-{:x}", Sha256::digest(&clean));
    let dir = crate::config_dir(app).join("wechat-avatars");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(format!("{id}.png"));
    if !path.exists() {
        let temp = dir.join(format!("{id}.next"));
        std::fs::write(&temp, &clean).map_err(|e| e.to_string())?;
        std::fs::rename(temp, path).map_err(|e| e.to_string())?;
    }
    Ok(id)
}
pub fn image_data(app: &tauri::AppHandle, id: &str) -> Result<String, String> {
    use base64::Engine;
    use std::io::Read;
    if !custom(id) {
        return Err("头像编号无效".into());
    }
    let file = std::fs::File::open(
        crate::config_dir(app)
            .join("wechat-avatars")
            .join(format!("{id}.png")),
    )
    .map_err(|_| "头像文件不存在")?;
    let mut bytes = vec![];
    file.take(300_001)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    let clean = clean_png(&bytes)?;
    Ok(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(clean)
    ))
}
