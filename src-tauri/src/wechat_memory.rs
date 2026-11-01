use serde::{Deserialize, Serialize};
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Memory {
    pub id: u64,
    pub key: String,
    pub kind: String,
    pub fact: String,
    pub source: String,
    pub evidence: String,
    pub updated_at: u64,
    pub active: bool,
    #[serde(default)]
    pub previous: Vec<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Update {
    pub key: String,
    pub kind: String,
    pub fact: String,
    pub source: String,
    pub evidence: String,
}
fn valid_text(text: &str, max: usize) -> bool {
    !text.trim().is_empty() && text.chars().count() <= max && !text.contains('\0')
}
fn apply_inner(
    items: &mut Vec<Memory>,
    updates: Vec<Update>,
    user: &str,
    story: &str,
    time: u64,
) -> Result<(), String> {
    if updates.len() > 4 {
        return Err("单轮最多更新4条记忆，本轮未保存".into());
    }
    for u in &updates {
        if !valid_text(&u.key, 40)
            || !u
                .key
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_')
            || ![
                "preference",
                "relationship",
                "promise",
                "event",
                "correction",
            ]
            .contains(&u.kind.as_str())
            || (["preference", "relationship", "promise", "correction"].contains(&u.kind.as_str())
                && u.source != "user")
            || !valid_text(&u.fact, 300)
            || !valid_text(&u.evidence, 400)
            || !match u.source.as_str() {
                "user" => user.contains(&u.evidence),
                "story" => story.contains(&u.evidence),
                _ => false,
            }
        {
            return Err("角色记忆缺少有效的原文依据，本轮未保存，请重试".into());
        }
    }
    for u in updates {
        if let Some(m) = items
            .iter_mut()
            .find(|m| m.key == u.key || m.active && m.kind == u.kind && m.fact == u.fact)
        {
            if m.fact != u.fact {
                m.previous.push(m.fact.clone());
                m.previous.drain(..m.previous.len().saturating_sub(8));
            }
            m.fact = u.fact;
            m.kind = u.kind;
            m.source = u.source;
            m.evidence = u.evidence;
            m.updated_at = time;
            m.active = true;
        } else {
            if items.len() >= 256 {
                return Err("记忆已达到256条，请在人物设定中整理后重试".into());
            }
            let id = items
                .iter()
                .map(|m| m.id)
                .max()
                .unwrap_or(0)
                .checked_add(1)
                .ok_or("记忆编号已达上限")?;
            items.push(Memory {
                id,
                key: u.key,
                kind: u.kind,
                fact: u.fact,
                source: u.source,
                evidence: u.evidence,
                updated_at: time,
                active: true,
                previous: vec![],
            });
        }
    }
    Ok(())
}
pub fn apply(
    items: &mut Vec<Memory>,
    updates: Vec<Update>,
    user: &str,
    story: &str,
    time: u64,
) -> Result<(), String> {
    let mut next = items.clone();
    apply_inner(&mut next, updates, user, story, time)?;
    *items = next;
    Ok(())
}
fn terms(text: &str) -> std::collections::BTreeSet<String> {
    let lower = text.to_lowercase();
    let chars: Vec<_> = lower
        .chars()
        .filter(|c| !c.is_whitespace() && !c.is_ascii_punctuation())
        .collect();
    chars.windows(2).map(|w| w.iter().collect()).collect()
}
pub fn recall(items: &[Memory], query: &str) -> Vec<Memory> {
    let query = terms(query);
    let mut scored: Vec<_> = items
        .iter()
        .filter(|m| m.active)
        .map(|m| {
            let hits = terms(&m.fact).intersection(&query).count();
            let importance = if ["preference", "promise", "correction"].contains(&m.kind.as_str()) {
                2
            } else {
                0
            };
            (hits * 10 + importance, m)
        })
        .filter(|(score, _)| *score > 0)
        .collect();
    scored.sort_by_key(|(score, m)| (std::cmp::Reverse(*score), std::cmp::Reverse(m.updated_at)));
    scored
        .into_iter()
        .take(8)
        .map(|(_, m)| {
            let mut value = m.clone();
            value.previous.clear();
            value
        })
        .collect()
}
pub fn edit(
    items: &mut [Memory],
    id: u64,
    text: String,
    active: bool,
    time: u64,
) -> Result<(), String> {
    if !valid_text(&text, 300) {
        return Err("记忆内容须为1–300字".into());
    }
    let m = items.iter_mut().find(|m| m.id == id).ok_or("记忆不存在")?;
    if m.fact != text {
        m.previous.push(m.fact.clone());
        m.previous.drain(..m.previous.len().saturating_sub(8));
    }
    m.fact = text;
    m.source = "user_edit".into();
    m.evidence = "用户在人物设定中修订".into();
    m.active = active;
    m.updated_at = time;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    fn update(fact: &str, evidence: &str) -> Update {
        Update {
            key: "coffee".into(),
            kind: "preference".into(),
            fact: fact.into(),
            source: "user".into(),
            evidence: evidence.into(),
        }
    }
    #[test]
    fn character_story_cannot_become_a_user_preference() {
        let u = Update {
            key: "tea".into(),
            kind: "preference".into(),
            fact: "用户喜欢茶".into(),
            source: "story".into(),
            evidence: "我喜欢茶".into(),
        };
        let mut items = vec![];
        assert!(apply(&mut items, vec![u], "你好", "我喜欢茶", 12).is_err());
        assert!(items.is_empty());
    }
    #[test]
    fn evidence_corrections_recall_and_forgetting_are_scoped() {
        let mut a = vec![];
        apply(
            &mut a,
            vec![update("喜欢咖啡", "喜欢咖啡")],
            "我喜欢咖啡",
            "",
            1,
        )
        .unwrap();
        let id = a[0].id;
        apply(
            &mut a,
            vec![update("现在喜欢茶", "喜欢茶")],
            "现在喜欢茶",
            "",
            2,
        )
        .unwrap();
        assert_eq!(a.len(), 1);
        assert_eq!(a[0].previous, vec!["喜欢咖啡"]);
        assert_eq!(recall(&a, "喜欢茶")[0].fact, "现在喜欢茶");
        assert!(recall(&a, "喜欢茶")[0].previous.is_empty());
        edit(&mut a, id, "不再喝茶".into(), false, 3).unwrap();
        assert!(recall(&a, "喝茶").is_empty());
        assert!(apply(
            &mut vec![],
            vec![update("错误", "不存在的原文")],
            "你好",
            "",
            4
        )
        .is_err());
        assert!(recall(&[], "咖啡").is_empty());
    }
}
