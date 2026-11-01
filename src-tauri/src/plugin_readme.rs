//! Human-readable package information, checked against the executable manifest.
use std::collections::BTreeMap;
pub const MAX_README: usize = 64 * 1024;
pub fn validate(text: &str, id: &str, name: &str, author: &str, version: &str, description: &str) -> Result<(), String> {
    if text.len()>MAX_README || text.contains('\0') {return Err("README.md 过大或包含无效字符".into());}
    let mut lines=text.trim_start_matches('\u{feff}').lines();
    if lines.next()!=Some("---") {return Err("README.md 必须以 --- 元信息区开始".into());}
    let mut fields=BTreeMap::new();let mut closed=false;
    for line in lines.by_ref() {
        if line=="---" {closed=true;break;}
        let (key,value)=line.split_once(':').ok_or("README.md 元信息须使用 key: value 格式")?;
        let key=key.trim();let value=value.trim();
        if !["id","name","author","version","description"].contains(&key) || fields.insert(key,value).is_some() {
            return Err("README.md 元信息包含未知或重复字段".into());
        }
    }
    if !closed || lines.collect::<Vec<_>>().join("\n").trim().is_empty() {return Err("README.md 缺少正文或元信息结束标记".into());}
    for (key,expected) in [("id",id),("name",name),("author",author),("version",version),("description",description)] {
        if expected.trim().is_empty() || fields.get(key).copied()!=Some(expected) {return Err(format!("README.md 的 {key} 缺失或与插件清单不一致"));}
    }
    Ok(())
}
#[cfg(test)] mod tests {
    use super::*;
    const GOOD:&str="---\nid: sample\nname: 示例\nauthor: 作者\nversion: 1.0.0\ndescription: 功能说明\n---\n# 示例\n工具功能说明。";
    #[test] fn required_information_matches_manifest() {
        assert!(validate(GOOD,"sample","示例","作者","1.0.0","功能说明").is_ok());
        for bad in [GOOD.replace("author: 作者\n",""),GOOD.replace("version: 1.0.0","version: 2.0.0"),GOOD.replace("id: sample","id: sample\nid: sample"),"# 没有元信息".into(),"x".repeat(MAX_README+1)] {
            assert!(validate(&bad,"sample","示例","作者","1.0.0","功能说明").is_err());
        }
    }
}
