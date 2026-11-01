use serde::{Deserialize, Serialize};
#[derive(Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Policy {
    pub moments_enabled: bool,
    pub chat_enabled: bool,
    pub quiet_start: u32,
    pub quiet_end: u32,
    pub interval_minutes: u64,
    pub chat_interval_minutes: u64,
    pub daily_moments: u32,
    pub daily_messages: u32,
}
impl Default for Policy {
    fn default() -> Self {
        Self {
            moments_enabled: true,
            chat_enabled: true,
            quiet_start: 23,
            quiet_end: 8,
            interval_minutes: 180,
            chat_interval_minutes: 15,
            daily_moments: 4,
            daily_messages: 2,
        }
    }
}
impl Policy {
    pub fn validate(&self) -> Result<(), String> {
        if self.quiet_start > 23
            || self.quiet_end > 23
            || self.quiet_start == self.quiet_end
            || !(60..=1440).contains(&self.interval_minutes)
            || !(5..=1440).contains(&self.chat_interval_minutes)
            || self.daily_moments > 12
            || self.daily_messages > 6
        {
            return Err("主动设置无效：间隔至少60分钟，私聊间隔至少5分钟，朋友圈每日最多12次，主动消息最多6次".into());
        }
        Ok(())
    }
    pub fn quiet(&self, hour: u32) -> bool {
        if self.quiet_start > self.quiet_end {
            hour >= self.quiet_start || hour < self.quiet_end
        } else {
            hour >= self.quiet_start && hour < self.quiet_end
        }
    }
}
#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Ledger {
    pub policy: Policy,
    pub attempts: Vec<Attempt>,
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Life {
    pub focus: String,
    pub progress: String,
    pub mood: String,
    pub last_event: String,
}
impl Life {
    pub fn validate(&self) -> Result<(), String> {
        if [&self.focus, &self.progress, &self.mood, &self.last_event]
            .iter()
            .any(|s| s.chars().count() > 300 || s.contains('\0'))
        {
            return Err("虚拟日常状态过长".into());
        }
        Ok(())
    }
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Attempt {
    pub at: u64,
    pub kind: String,
    pub status: String,
    pub reason: String,
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Social {
    pub last_response_silent: bool,
    pub next_topic: String,
    pub paused: bool,
    pub unread: u32,
    pub attempts: Vec<Attempt>,
    pub unanswered: u32,
    pub last_user_at: u64,
    pub life: Life,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Action {
    #[serde(default)]
    pub next_topic: Option<String>,
    #[serde(default)]
    pub reactions: Vec<crate::wechat_moments::Reaction>,
    pub message: Option<String>,
    pub moment: Option<String>,
    #[serde(default)]
    pub life: Option<Life>,
    #[serde(default)]
    pub reason: String,
}
pub fn parse(text: &str, kind: &str) -> Result<Action, String> {
    let text = text.trim();
    let text = text
        .strip_prefix("```json")
        .or_else(|| text.strip_prefix("```"))
        .and_then(|s| s.trim().strip_suffix("```"))
        .unwrap_or(text)
        .trim();
    let mut a: Action =
        serde_json::from_str(text).map_err(|_| "主动内容格式无效，未发布".to_string())?;
    if a.next_topic
        .as_ref()
        .is_some_and(|t| t.chars().count() > 500 || t.contains('\0'))
    {
        return Err("待聊话题过长".into());
    }
    if a.reactions.len() > 3 {
        return Err("每次最多3项朋友圈互动".into());
    }
    if a.reason.chars().count() > 300 {
        return Err("主动理由过长".into());
    }
    if let Some(v) = &mut a.message {
        *v = v.trim().into();
        if kind != "message"
            || v.is_empty()
            || v.chars().count() > 800
            || v.contains("【状态更新】")
            || v.contains("【叙事】")
        {
            return Err("主动消息不符合直接聊天格式".into());
        }
    }
    if let Some(v) = &mut a.moment {
        if kind != "moment" {
            return Err("本次只允许主动聊天".into());
        }
        *v = crate::wechat_moments::validate(v)?;
    }
    if let Some(life) = &a.life {
        life.validate()?;
    }
    Ok(a)
}
pub fn contact_control(text: &str) -> Option<bool> {
    match text.trim().trim_end_matches(['。', '！', '!', ' ']) {
        "不要主动给我发消息"
        | "不要主动找我"
        | "别主动找我"
        | "别打扰我"
        | "不要打扰我"
        | "先别打扰我"
        | "停止主动联系" => Some(true),
        "可以主动找我了" | "恢复主动联系" | "可以主动给我发消息" => {
            Some(false)
        }
        _ => None,
    }
}
pub fn day(at: u64) -> String {
    use chrono::TimeZone;
    chrono::Local
        .timestamp_opt(at.min(i64::MAX as u64) as i64, 0)
        .single()
        .map(|d| d.format("%Y-%m-%d").to_string())
        .unwrap_or_default()
}
pub fn allowed(
    session: &crate::wechat_story::Session,
    policy: &Policy,
    kind: &str,
    time: u64,
    today: &str,
) -> bool {
    if session.deleted_at > 0
        || session.messages.is_empty()
            && session.card.personality.trim().is_empty()
            && session.card.interests.trim().is_empty()
    {
        return false;
    }
    if time.saturating_sub(session.social.last_user_at.max(session.updated_at))
        < if kind == "message" { 120 } else { 300 }
    {
        return false;
    }
    if session
        .social
        .attempts
        .iter()
        .rev()
        .find(|a| a.kind == kind)
        .map(|a| {
            time.saturating_sub(a.at)
                < if kind == "message" {
                    policy.chat_interval_minutes * 60
                } else {
                    policy.interval_minutes * 60
                }
        })
        .unwrap_or(false)
    {
        return false;
    }
    // A deliberate pause must not be immediately undone by an idle check.
    if kind == "message"
        && session.social.last_response_silent
        && time.saturating_sub(session.social.last_user_at) < policy.chat_interval_minutes * 60
    {
        return false;
    }
    if kind == "message" && (session.social.unanswered >= 1 || session.social.paused) {
        return false;
    }
    if kind == "moment"
        && session
            .social
            .attempts
            .iter()
            .any(|a| a.kind == kind && a.status == "published" && day(a.at) == today)
    {
        return false;
    }
    true
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn deliberate_silence_does_not_immediately_trigger_a_proactive_followup() {
        let p = Policy::default();
        let mut s = crate::wechat_story::Session::default();
        s.card.personality = "安静".into();
        s.social.last_response_silent = true;
        s.social.last_user_at = 100000;
        assert!(!allowed(&s, &p, "message", 100200, "today"));
        assert!(allowed(&s, &p, "message", 101000, "today"));
    }
    #[test]
    fn chat_has_its_own_cooldown_and_can_resume_after_user_response() {
        let policy = Policy::default();
        let time = 1800000000;
        let today = day(time);
        let mut s = crate::wechat_story::Session::default();
        s.card.interests = "阅读".into();
        s.social.attempts.push(Attempt {
            at: time - 60,
            kind: "moment".into(),
            status: "published".into(),
            reason: String::new(),
        });
        assert!(allowed(&s, &policy, "message", time, &today));
        assert!(!allowed(&s, &policy, "moment", time, &today));
        s.social.attempts.push(Attempt {
            at: time - 60,
            kind: "message".into(),
            status: "published".into(),
            reason: String::new(),
        });
        assert!(!allowed(&s, &policy, "message", time, &today));
        s.social.unanswered = 1;
        assert!(!allowed(&s, &policy, "message", time + 1000, &today));
        s.social.unanswered = 0;
        s.social.last_user_at = time + 800;
        assert!(allowed(&s, &policy, "message", time + 1000, &today));
        let old: Policy =
            serde_json::from_str(r#"{"interval_minutes":180,"daily_messages":2}"#).unwrap();
        assert_eq!(old.chat_interval_minutes, 15);
        assert_eq!(old.daily_messages, 2);
    }
    #[test]
    fn quiet_hours_cooldown_unanswered_and_deleted_roles_block_activity() {
        let p = Policy::default();
        assert!(p.quiet(23));
        assert!(p.quiet(7));
        assert!(!p.quiet(12));
        let mut s = crate::wechat_story::Session::default();
        s.card.personality = "温和".into();
        assert!(allowed(&s, &p, "moment", 100000, "today"));
        s.social.paused = true;
        assert!(!allowed(&s, &p, "message", 100000, "today"));
        s.social.paused = false;
        s.social.unanswered = 1;
        assert!(!allowed(&s, &p, "message", 100000, "today"));
        s.deleted_at = 1;
        assert!(!allowed(&s, &p, "moment", 100000, "today"));
        s.deleted_at = 0;
        s.social.attempts.push(Attempt {
            at: 99999,
            kind: "moment".into(),
            status: "skipped".into(),
            reason: "休息".into(),
        });
        assert!(!allowed(&s, &p, "moment", 100000, "today"));
    }
    #[test]
    fn explicit_contact_controls_do_not_treat_quotes_as_authorization() {
        assert_eq!(contact_control("别主动找我。"), Some(true));
        assert_eq!(contact_control("恢复主动联系"), Some(false));
        assert_eq!(contact_control("他说：别打扰我"), None);
    }
    #[test]
    fn model_can_rest_and_cannot_publish_to_another_channel() {
        assert!(parse(
            r#"{"message":null,"moment":null,"reason":"暂时没有新内容"}"#,
            "moment"
        )
        .is_ok());
        assert!(parse(r#"{"message":"打扰一下","moment":null}"#, "moment").is_err());
    }
}
