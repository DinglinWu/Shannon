//! Bounded creative character model. Numbers are narrative tendencies, not a psychological test.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
pub const TRAITS: &[&str] = &["social", "direct", "order", "warmth", "curiosity", "steady"];
const MOOD: &[&str] = &["tension", "pleasure", "energy"];
const RELATION: &[&str] = &["trust", "familiarity", "guard"];
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Speech {
    Short,
    Formal,
    Casual,
    DryHumor,
    Playful,
    FewQuestions,
    Blunt,
    Indirect,
    Reserved,
    Expressive,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Profile {
    pub traits: BTreeMap<String, u8>,
    pub speech: BTreeSet<Speech>,
    pub anchors: BTreeMap<String, String>,
    pub relations: BTreeMap<String, BTreeMap<String, u8>>,
    pub mood: BTreeMap<String, u8>,
    pub needs_review: bool,
    pub last_growth: BTreeMap<String, u64>,
    pub growth_evidence: Vec<String>,
}
#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Author,
    Growth,
    Scene,
    Migrate,
}
#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    Input,
    Narrative,
    World,
    Legacy,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Update {
    pub name: String,
    pub mode: Mode,
    pub source: Source,
    pub evidence: String,
    pub reason: String,
    #[serde(default)]
    pub traits: BTreeMap<String, Option<u8>>,
    #[serde(default)]
    pub speech_add: Vec<Speech>,
    #[serde(default)]
    pub speech_remove: Vec<Speech>,
    #[serde(default)]
    pub anchors: BTreeMap<String, Option<String>>,
    #[serde(default)]
    pub relations: BTreeMap<String, BTreeMap<String, Option<u8>>>,
    #[serde(default)]
    pub mood: BTreeMap<String, Option<u8>>,
}
fn check_map(values: &BTreeMap<String, u8>, keys: &[&str]) -> Result<(), String> {
    if values
        .iter()
        .any(|(key, value)| !keys.contains(&key.as_str()) || *value > 100)
    {
        Err("人物数值维度或范围无效（仅0–100）".into())
    } else {
        Ok(())
    }
}
impl Profile {
    pub fn validate(&self) -> Result<(), String> {
        if self.last_growth.len() > 7
            || self
                .last_growth
                .keys()
                .any(|k| !TRAITS.contains(&k.as_str()) && k != "speech")
            || self.growth_evidence.len() > 8
        {
            return Err("人物成长记录超过固定容量".into());
        }
        check_map(&self.traits, TRAITS)?;
        check_map(&self.mood, MOOD)?;
        if self.speech.len() > 6 || self.anchors.len() > 8 || self.relations.len() > 12 {
            return Err("人物模型超过固定容量，先明确修订已有条目".into());
        }
        for (key, value) in &self.anchors {
            if key.is_empty()
                || key.len() > 32
                || !key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
                || value.trim().is_empty()
                || value.chars().count() > 48
                || value.contains('\0')
            {
                return Err("人物短背景须使用稳定key，正文不超过48字".into());
            }
        }
        for (name, r) in &self.relations {
            if name.trim().is_empty() || name.chars().count() > 40 {
                return Err("关系人物姓名无效".into());
            }
            check_map(r, RELATION)?;
        }
        Ok(())
    }
    pub fn compact(&self, related: &[&str]) -> Value {
        let relations: BTreeMap<_, _> = self
            .relations
            .iter()
            .filter(|(name, _)| related.contains(&name.as_str()))
            .take(3)
            .collect();
        json!({"traits":self.traits,"speech":self.speech,"anchors":self.anchors,"relations":relations,"mood":self.mood})
    }
}
fn patch(
    values: &mut BTreeMap<String, u8>,
    changes: &BTreeMap<String, Option<u8>>,
    keys: &[&str],
    max_step: Option<u8>,
    allow_clear: bool,
) -> Result<(), String> {
    for (key, value) in changes {
        if !keys.contains(&key.as_str()) {
            return Err("未知人物数值维度".into());
        }
        if let Some(value) = value {
            if *value > 100 {
                return Err("人物数值必须为0–100".into());
            }
            if let Some(step) = max_step {
                let old = values
                    .get(key)
                    .ok_or("未知性格维度须先根据用户设定初始化，不能从中立值猜起")?;
                if old.abs_diff(*value) > step {
                    return Err(format!("人物成长单轮最多变化{step}点"));
                }
            }
            values.insert(key.clone(), *value);
        } else if allow_clear {
            values.remove(key);
        } else {
            return Err("临时反应不能删除稳定人设".into());
        }
    }
    Ok(())
}
pub fn apply(
    current: Option<&Profile>,
    u: &Update,
    source_text: &str,
    valid_names: &[&str],
    round: u64,
) -> Result<Profile, String> {
    if u.evidence.trim().is_empty() || u.evidence.chars().count() > 240 {
        return Err("人物更新须引用1–240字的连续原文片段".into());
    }
    if !source_text.contains(&u.evidence) {
        return Err("人物更新引用与指定来源不符，须逐字引用，不改写或拼接".into());
    }
    if u.reason.trim().is_empty() || u.reason.chars().count() > 160 {
        return Err("人物更新须填写1–160字的简短变化依据".into());
    }
    let mut p = current.cloned().unwrap_or_default();
    let stable = !u.traits.is_empty()
        || !u.speech_add.is_empty()
        || !u.speech_remove.is_empty()
        || !u.anchors.is_empty();
    if !stable && u.relations.is_empty() && u.mood.is_empty() && u.mode != Mode::Migrate {
        return Err("没有需要更新的人物维度".into());
    }
    match u.mode {
        Mode::Author if u.source != Source::Input => {
            return Err("底色修订须引用本轮用户创作".into())
        }
        Mode::Migrate if current.is_some_and(|p| !p.needs_review) => {
            return Err("人物已整理，不能反复用旧描述覆盖".into())
        }
        Mode::Migrate if ![Source::World, Source::Legacy, Source::Input].contains(&u.source) => {
            return Err("旧人物整理不能引用模型新编剧情".into())
        }
        Mode::Scene if stable => return Err("临时情绪不能改变底色".into()),
        Mode::Scene if ![Source::Input, Source::Narrative].contains(&u.source) => {
            return Err("临时状态须引用本轮输入或叙事".into())
        }
        Mode::Growth if u.source != Source::Narrative && u.source != Source::Input => {
            return Err("人物成长须有本轮经历依据".into())
        }
        _ => {}
    }
    if u.mode == Mode::Growth && current.is_none() {
        return Err("请先整理已有用户资料，再更新人物状态".into());
    }
    if !matches!(u.mode, Mode::Author | Mode::Migrate) && !u.anchors.is_empty() {
        return Err("不能把模型推测固化为背景或例外".into());
    }
    if u.mode == Mode::Growth {
        if !stable {
            return Err("情绪和关系变化请使用scene，不必改动性格".into());
        }
        let fingerprint = crate::plugin_package::digest(u.evidence.as_bytes());
        if p.growth_evidence.contains(&fingerprint) {
            return Err("同一条经历不能反复推动性格数值".into());
        }
        let mut keys: Vec<_> = u.traits.keys().map(String::as_str).collect();
        if !u.speech_add.is_empty() || !u.speech_remove.is_empty() {
            keys.push("speech");
        }
        if keys.iter().any(|k| {
            p.last_growth
                .get(*k)
                .is_some_and(|last| round.saturating_sub(*last) < 6)
        }) {
            return Err("稳定性格在六轮内不重复调整，临时变化请更新情绪或关系".into());
        }
        for key in keys {
            p.last_growth.insert(key.into(), round);
        }
        p.growth_evidence.push(fingerprint);
        if p.growth_evidence.len() > 8 {
            p.growth_evidence.remove(0);
        }
    }
    let author = matches!(u.mode, Mode::Author | Mode::Migrate);
    patch(
        &mut p.traits,
        &u.traits,
        TRAITS,
        if u.mode == Mode::Growth {
            Some(3)
        } else {
            None
        },
        author,
    )?;
    if u.mode == Mode::Growth && (u.speech_add.len() > 1 || u.speech_remove.len() > 1) {
        return Err("单轮习惯成长最多增删各一项".into());
    }
    if u.speech_add.iter().any(|v| u.speech_remove.contains(v)) {
        return Err("同一说话习惯不能同时新增和删除".into());
    }
    for tag in &u.speech_remove {
        p.speech.remove(tag);
    }
    for tag in &u.speech_add {
        p.speech.insert(*tag);
    }
    for (key, value) in &u.anchors {
        if let Some(value) = value {
            p.anchors.insert(key.clone(), value.clone());
        } else {
            p.anchors.remove(key);
        }
    }
    for (name, changes) in &u.relations {
        if !valid_names.contains(&name.as_str()) || name == &u.name {
            return Err("关系只能指向故事中其他人物".into());
        }
        let r = p.relations.entry(name.clone()).or_default();
        // Established relationships change slowly; first initialization can reflect existing history.
        for (key, value) in changes {
            let max_step = if !author && r.contains_key(key) {
                Some(10)
            } else {
                None
            };
            patch(
                r,
                &BTreeMap::from([(key.clone(), *value)]),
                RELATION,
                max_step,
                author,
            )?;
        }
        if r.is_empty() {
            p.relations.remove(name);
        }
    }
    patch(&mut p.mood, &u.mood, MOOD, None, true)?;
    // Transient updates must not consume pending migration of legacy references.
    p.needs_review = if u.mode == Mode::Migrate {
        false
    } else {
        current.map_or(!stable, |old| old.needs_review)
    };
    p.validate()?;
    Ok(p)
}

pub const RULES: &str = r#"人物使用固定容量的混合模型，不是心理测评：traits均为0–100的创作倾向，social=外向社交、direct=表达直接、order=自律条理、warmth=温和体贴、curiosity=好奇开放、steady=情绪稳健；缺失表示未知，不能默认50或凭一次反应补齐。speech只能用short短句/formal正式/casual随意/dry_humor冷幽默/playful俏皮/few_questions少反问/blunt直白/indirect委婉/reserved克制/expressive外露，最多6项；它与性格数值互补，不把分数当必须执行的台词模板。anchors是最多8条各48字的关键背景、动机、习惯例外或用户台词示例，使用稳定ASCII key；不存长篇心理解释。relations按对象分开记录trust信任/familiarity熟悉/guard戒备，mood仅tension紧张/pleasure愉快/energy精力，均0–100。不同对象的关系不能互相覆盖，不用当时情绪改写稳定倾向。
输出profile_updates数组（无变化为空，每轮每人最多1条，共16条）。格式：{"name":"人物名","mode":"author或growth或scene或migrate","source":"input或narrative或world或legacy","evidence":"来源的逐字片段，最多240字","reason":"简短变化依据，最多160字","traits":{"direct":65},"speech_add":["dry_humor"],"speech_remove":[],"anchors":{"background":"简短事实"},"relations":{"其他人物":{"trust":55}},"mood":{"tension":30}}。所有维度只填本轮实际变化的key，省略保持，绝不整表重写；author/migrate允许null撤销明确误读的维度或anchor。容量满时不要自动淘汰旧特点，可暂不存新anchor，重要背景仍可记入有依据的长期记忆。
author：用户本轮对该人物的反应、台词、背景、创作期望有明确新线索时，引用input渐进修订，不需要专门口令。仅调整受影响维度，保留无关特点；一次生气/脸红/暂时拒绝只影响mood，不能概括成永久性格。主控角色的猜测、玩笑、传闻不自动当真。
migrate：当profile_initialization_required=true时，本轮必须优先为该人物提交一条migrate更新，参考已有用户资料谨慎整理一次，source为input/world/legacy；legacy指当前人物旧的personality/description/goal/relationships原文。只归纳有依据的倾向，未知留空，不编造喜好、经历或标签。整理后不每轮再总结长文；已有profile不重复迁移，设置被用户修改后才根据legacy_seed局部校正。
growth：仅重要新经历可缓慢改变已有traits，单轮最多3点，同维度六轮内最多一次，习惯最多增删各1项；不能用重复一句经历每轮加分，不能用AI叙事补造背景锚点。情绪/关系通常用scene，不是性格成长。source须本轮input或narrative。author明确纠正不受步长限制。
scene：引用input或narrative，只改mood与relations，稳定人设保持；尚未初始化也可保存临时状态，但仍须后续整理。待初始化时优先使用migrate，可在同条更新中附上有据的mood与relations；已知关系单轮最多10点。换场应用清空mood，不清理traits、speech、anchors和relations。新场景需要的情绪可以同轮写入。
旧personality等长文只作本地原始资料，不再通过author_updates/personality整段改写来驱动角色；普通states只填确实变化的在场状态和简短行动，不重复输出心理分析。profile已覆盖的分数和习惯不要再写入memory_updates；关键事件和重要背景才写长期记忆。数字是倾向，不是命令，结合本轮对话、情境与背景自然回应。"#;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scene_before_migration_preserves_pending_references_and_saved_state() {
        let scene = update(json!({"mode":"scene","mood":{"tension":70},"relations":{"我":{"trust":40}}}));
        let p = apply(None, &scene, "用户描述", &["我"], 1).unwrap();
        assert!(p.needs_review);
        assert!(p.traits.is_empty());
        let p = apply(Some(&p), &scene, "用户描述", &["我"], 2).unwrap();
        assert!(p.needs_review);
        let p = apply(Some(&p), &update(json!({"traits":{"direct":65}})), "用户描述", &["我"], 3).unwrap();
        assert!(p.needs_review);
        let p = apply(Some(&p), &update(json!({"mode":"migrate","source":"legacy","traits":{"order":75}})), "用户描述", &["我"], 4).unwrap();
        assert!(!p.needs_review);
        assert_eq!(p.traits["direct"], 65);
        assert_eq!(p.traits["order"], 75);
        assert_eq!(p.mood["tension"], 70);
        assert_eq!(p.relations["我"]["trust"], 40);
        assert!(apply(None, &update(json!({"mode":"growth","traits":{"direct":65}})), "用户描述", &["我"], 1).is_err());
    }
    fn update(value: Value) -> Update {
        let mut base = json!({"name":"小林","mode":"author","source":"input","evidence":"用户描述","reason":"用户补充设定"});
        for (k, v) in value.as_object().unwrap() {
            base[k] = v.clone();
        }
        serde_json::from_value(base).unwrap()
    }
    #[test]
    fn sparse_author_updates_keep_all_unmentioned_traits_habits_and_background() {
        let p=apply(None,&update(json!({"traits":{"direct":70,"social":30},"speech_add":["short"],"anchors":{"home":"海边长大"}})),"用户描述",&["我","小林"],1).unwrap();
        let next = apply(
            Some(&p),
            &update(json!({"traits":{"curiosity":80},"speech_add":["dry_humor"]})),
            "用户描述",
            &["我", "小林"],
            2,
        )
        .unwrap();
        assert_eq!(next.traits["direct"], 70);
        assert_eq!(next.traits["social"], 30);
        assert_eq!(next.anchors["home"], "海边长大");
        assert!(next.speech.contains(&Speech::Short));
        assert!(next.speech.contains(&Speech::DryHumor));
        assert!(!next.traits.contains_key("warmth"));
        let corrected = apply(
            Some(&next),
            &update(json!({"traits":{"direct":20},"speech_remove":["short"]})),
            "用户描述",
            &["我", "小林"],
            3,
        )
        .unwrap();
        assert_eq!(corrected.traits["direct"], 20);
        assert!(corrected.speech.contains(&Speech::DryHumor));
        assert!(!corrected.speech.contains(&Speech::Short));
    }
    #[test]
    fn emotions_cannot_rewrite_core_and_growth_is_slow_and_not_replayed() {
        let p = apply(
            None,
            &update(json!({"traits":{"steady":60}})),
            "用户描述",
            &["我", "小林"],
            1,
        )
        .unwrap();
        assert!(apply(
            Some(&p),
            &update(json!({"mode":"scene","traits":{"steady":90}})),
            "用户描述",
            &["我"],
            2
        )
        .is_err());
        assert!(apply(
            Some(&p),
            &update(json!({"mode":"growth","traits":{"steady":70}})),
            "用户描述",
            &["我"],
            2
        )
        .is_err());
        let grown = apply(
            Some(&p),
            &update(json!({"mode":"growth","traits":{"steady":63}})),
            "用户描述",
            &["我"],
            2,
        )
        .unwrap();
        assert!(apply(
            Some(&grown),
            &update(json!({"mode":"growth","evidence":"新经历","traits":{"steady":65}})),
            "新经历",
            &["我"],
            3
        )
        .is_err());
        assert!(apply(
            Some(&grown),
            &update(json!({"mode":"growth","traits":{"steady":65}})),
            "用户描述",
            &["我"],
            10
        )
        .is_err());
        assert!(apply(
            Some(&grown),
            &update(json!({"mode":"growth","evidence":"新经历","traits":{"steady":65}})),
            "新经历",
            &["我"],
            10
        )
        .is_ok());
        assert!(apply(
            Some(&p),
            &update(json!({"mode":"growth","traits":{"social":50}})),
            "用户描述",
            &["我"],
            2
        )
        .is_err());
    }
    #[test]
    fn strict_dimensions_and_capacity_reject_bad_updates_without_losing_old_data() {
        for values in [
            json!({"traits":{"unknown":20}}),
            json!({"traits":{"direct":101}}),
            json!({"anchors":{"bad-key":"bad"}}),
        ] {
            assert!(apply(None, &update(values), "用户描述", &["我"], 1).is_err());
        }
        let mut p = Profile::default();
        for i in 0..8 {
            p.anchors.insert(format!("key{i}"), "保留原有特点".into());
        }
        let before = p.clone();
        assert!(apply(
            Some(&p),
            &update(json!({"anchors":{"new":"新特点"}})),
            "用户描述",
            &["我"],
            1
        )
        .is_err());
        assert_eq!(p, before);
        assert!(apply(
            None,
            &update(json!({"traits":{"direct":60}})),
            "没有原文",
            &["我"],
            1
        )
        .is_err());
    }
    #[test]
    fn relationships_are_separate_and_compact_context_never_includes_growth_provenance() {
        let p = apply(
            None,
            &update(json!({"relations":{"我":{"trust":50},"小雨":{"trust":80}}})),
            "用户描述",
            &["我", "小雨"],
            1,
        )
        .unwrap();
        let next = apply(
            Some(&p),
            &update(json!({"mode":"scene","relations":{"我":{"trust":60}},"mood":{"tension":75}})),
            "用户描述",
            &["我", "小雨"],
            2,
        )
        .unwrap();
        assert_eq!(next.relations["小雨"]["trust"], 80);
        assert_eq!(next.mood["tension"], 75);
        assert!(apply(
            Some(&next),
            &update(json!({"mode":"scene","relations":{"我":{"trust":90}}})),
            "用户描述",
            &["我"],
            3
        )
        .is_err());
        let view = next.compact(&["我"]);
        assert!(view["relations"].get("小雨").is_none());
        assert!(view.get("growth_evidence").is_none());
        assert!(view.get("last_growth").is_none());
        assert!(view.to_string().chars().count() < 1600);
    }
    #[test]
    fn migration_only_runs_once_unless_user_reference_changes() {
        let init = update(json!({"mode":"migrate","source":"world","traits":{"direct":65}}));
        let p = apply(None, &init, "用户描述", &[], 1).unwrap();
        assert!(apply(Some(&p), &init, "用户描述", &[], 2).is_err());
        let mut pending = p;
        pending.needs_review = true;
        assert!(apply(Some(&pending), &init, "用户描述", &[], 2).is_ok());
    }
}
