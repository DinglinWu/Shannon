//! Deliberately bounded JSON Schema subset for plugin tool arguments.
use serde_json::Value;

pub fn check(schema: &Value) -> Result<(), String> {
    check_at(schema, 0)
}
fn check_at(s: &Value, depth: usize) -> Result<(), String> {
    if depth > 8 {
        return Err("参数 Schema 嵌套超过8层".into());
    }
    let object = s.as_object().ok_or("参数 Schema 必须是对象")?;
    for key in object.keys() {
        if ![
            "type",
            "description",
            "properties",
            "required",
            "additionalProperties",
            "items",
            "enum",
            "minimum",
            "maximum",
            "minLength",
            "maxLength",
            "minItems",
            "maxItems",
        ]
        .contains(&key.as_str())
        {
            return Err(format!("不支持的 Schema 字段：{key}"));
        }
    }
    let kind = s["type"].as_str().ok_or("Schema 缺少 type")?;
    if ![
        "object", "array", "string", "integer", "number", "boolean", "null",
    ]
    .contains(&kind)
    {
        return Err("不支持的参数类型".into());
    }
    if kind != "object"
        && ["properties", "required", "additionalProperties"]
            .iter()
            .any(|key| s.get(key).is_some())
    {
        return Err("对象 Schema 字段与类型不符".into());
    }
    if let Some(v) = s.get("description") {
        if v.as_str().map_or(true, |t| t.len() > 2000) {
            return Err("Schema 描述无效".into());
        }
    }
    if let Some(v) = s.get("enum") {
        if v.as_array().map_or(true, |a| a.is_empty() || a.len() > 64) {
            return Err("enum 必须包含1–64个值".into());
        }
    }
    for key in ["minimum", "maximum"] {
        if let Some(v) = s.get(key) {
            if !["integer", "number"].contains(&kind) || !v.is_number() {
                return Err(format!("{key} 仅适用于数值"));
            }
        }
    }
    for (key, expected) in [
        ("minLength", "string"),
        ("maxLength", "string"),
        ("minItems", "array"),
        ("maxItems", "array"),
    ] {
        if let Some(v) = s.get(key) {
            if kind != expected || v.as_u64().map_or(true, |n| n > 32768) {
                return Err(format!("{key} 范围无效"));
            }
        }
    }
    for (low, high) in [
        ("minimum", "maximum"),
        ("minLength", "maxLength"),
        ("minItems", "maxItems"),
    ] {
        if let (Some(a), Some(b)) = (s[low].as_f64(), s[high].as_f64()) {
            if a > b {
                return Err("Schema 上下界无效".into());
            }
        }
    }
    if kind == "object" {
        if s["additionalProperties"] != false {
            return Err("对象参数必须声明 additionalProperties:false".into());
        }
        let props = s["properties"].as_object().ok_or("对象缺少 properties")?;
        if props.len() > 32 {
            return Err("对象字段超过32个".into());
        }
        for (name, child) in props {
            if name.is_empty() || name.len() > 64 {
                return Err("参数名无效".into());
            }
            check_at(child, depth + 1)?;
        }
        if let Some(required) = s.get("required") {
            let required = required.as_array().ok_or("required 必须是数组")?;
            let mut seen = std::collections::HashSet::new();
            for key in required {
                let key = key.as_str().ok_or("required 字段无效")?;
                if !props.contains_key(key) || !seen.insert(key) {
                    return Err("required 字段不存在或重复".into());
                }
            }
        }
    } else if kind == "array" {
        check_at(s.get("items").ok_or("数组缺少 items")?, depth + 1)?;
    } else if ["properties", "required", "additionalProperties", "items"]
        .iter()
        .any(|key| s.get(key).is_some())
    {
        return Err("Schema 字段与类型不符".into());
    }
    if kind != "array" && s.get("items").is_some() {
        return Err("items 仅适用于数组".into());
    }
    Ok(())
}

pub fn validate(schema: &Value, value: &Value) -> Result<(), String> {
    if value.to_string().len() > 32768 {
        return Err("插件参数超过32KiB".into());
    }
    validate_at(schema, value, "参数")
}
fn validate_at(s: &Value, v: &Value, path: &str) -> Result<(), String> {
    let kind = s["type"].as_str().unwrap_or("");
    let typed = match kind {
        "object" => v.is_object(),
        "array" => v.is_array(),
        "string" => v.is_string(),
        "integer" => v.is_i64() || v.is_u64(),
        "number" => v.is_number(),
        "boolean" => v.is_boolean(),
        "null" => v.is_null(),
        _ => false,
    };
    if !typed {
        return Err(format!("{path} 类型应为 {kind}"));
    }
    if let Some(choices) = s["enum"].as_array() {
        if !choices.contains(v) {
            return Err(format!("{path} 不在允许值中"));
        }
    }
    if let Some(object) = v.as_object() {
        let props = s["properties"].as_object().ok_or("Schema 无效")?;
        if let Some(required) = s["required"].as_array() {
            for key in required {
                let key = key.as_str().ok_or("Schema 无效")?;
                if !object.contains_key(key) {
                    return Err(format!("{path} 缺少 {key}"));
                }
            }
        }
        for (key, value) in object {
            validate_at(
                props
                    .get(key)
                    .ok_or_else(|| format!("{path} 不允许字段 {key}"))?,
                value,
                &format!("{path}.{key}"),
            )?;
        }
    }
    if let Some(array) = v.as_array() {
        bounds(s, array.len() as f64, "minItems", "maxItems", path)?;
        for item in array {
            validate_at(&s["items"], item, path)?;
        }
    }
    if let Some(text) = v.as_str() {
        bounds(
            s,
            text.chars().count() as f64,
            "minLength",
            "maxLength",
            path,
        )?;
    }
    if let Some(number) = v.as_f64() {
        bounds(s, number, "minimum", "maximum", path)?;
    }
    Ok(())
}
fn bounds(s: &Value, n: f64, min: &str, max: &str, path: &str) -> Result<(), String> {
    if s[min].as_f64().map_or(false, |v| n < v) || s[max].as_f64().map_or(false, |v| n > v) {
        return Err(format!("{path} 超出范围"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn schemas_and_arguments_are_bounded() {
        let s = json!({"type":"object","properties":{"n":{"type":"integer","minimum":1,"maximum":8}},"required":["n"],"additionalProperties":false});
        check(&s).unwrap();
        validate(&s, &json!({"n":3})).unwrap();
        for bad in [
            json!({}),
            json!({"n":-1}),
            json!({"n":1.5}),
            json!({"n":9}),
            json!({"n":3,"extra":true}),
        ] {
            assert!(validate(&s, &bad).is_err());
        }
        for bad in [
            json!({"$ref":"x"}),
            json!({"type":"object","properties":{}}),
            json!({"type":"array"}),
            json!({"type":"string","maximum":3}),
        ] {
            assert!(check(&bad).is_err());
        }
        assert!(check(&json!({"type":"array","items":{"type":"string"},"properties":{}})).is_err());
    }
}
