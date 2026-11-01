//! Bounded streaming tool loop. Provider adapters never execute application code.
use std::collections::BTreeMap;
use futures_util::StreamExt;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter};
use crate::{ai_tools, config::Config};

const MAX_ROUNDS: usize = 6;
const MAX_CALLS: usize = 16;
const MAX_ARGUMENT_BYTES: usize = 32768;

#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct ProtocolTurn { pub provider: String, pub messages: Vec<Value> }
pub struct Answer { pub text: String, pub protocol: ProtocolTurn }

pub fn history(messages: &[crate::ChatMessage], anthropic: bool) -> Vec<Value> {
    let provider = if anthropic { "anthropic" } else { "openai" };
    messages.iter().flat_map(|m| {
        if let Some(turn) = &m.protocol {
            if turn.provider == provider { return turn.messages.clone(); }
        }
        let mut message = json!({"role":m.role,"content":m.content});
        if !anthropic && m.role == "assistant" { message["reasoning_content"] = json!(""); }
        vec![message]
    }).collect()
}

fn normalize_blocks(blocks: &mut BTreeMap<usize, Value>) -> Result<(), String> {
    for block in blocks.values_mut() {
        if let Some(raw) = block.as_object_mut().and_then(|o| o.remove("_partial_input")) {
            block["input"] = serde_json::from_str(raw.as_str().unwrap_or("")).map_err(|_| "搜索工具参数无效")?;
        }
    }
    Ok(())
}

#[derive(Default, Clone)]
struct Call { id: String, name: String, arguments: String }
#[derive(Default)]
struct Round {
    text: String,
    reasoning: String,
    calls: BTreeMap<usize, Call>,
    blocks: BTreeMap<usize, Value>,
    finish: String,
    complete: bool,
}

fn append(target: &mut String, delta: &Value) -> Result<(), String> {
    if let Some(text) = delta.as_str() { target.push_str(text); }
    if target.len() > MAX_ARGUMENT_BYTES { return Err("工具参数超出长度限制".into()); }
    Ok(())
}

impl Round {
    fn event(&mut self, v: &Value, anthropic: bool) -> Result<(Option<String>, Option<String>), String> {
        let mut text = None;
        let mut thought = None;
        if let Some(error) = v.get("error") { return Err(error.to_string()); }
        if !anthropic {
            let choice = &v["choices"][0];
            if let Some(reason) = choice["finish_reason"].as_str() { self.finish = reason.into(); }
            let d = &choice["delta"];
            text = d["content"].as_str().map(str::to_owned);
            thought = d["reasoning_content"].as_str().map(str::to_owned);
            if let Some(calls) = d["tool_calls"].as_array() {
                for part in calls {
                    let index = part["index"].as_u64().ok_or("工具调用缺少index")? as usize;
                    if index >= MAX_CALLS { return Err("工具调用数量过多".into()); }
                    let call = self.calls.entry(index).or_default();
                    append(&mut call.id, &part["id"])?;
                    append(&mut call.name, &part["function"]["name"])?;
                    append(&mut call.arguments, &part["function"]["arguments"])?;
                }
            }
        } else {
            let index = v["index"].as_u64().unwrap_or(0) as usize;
            if index > 256 { return Err("回复内容块过多".into()); }
            match v["type"].as_str() {
                Some("content_block_start") => {
                    let block = v["content_block"].clone();
                    if block["type"] == "tool_use" {
                        if self.calls.len() >= MAX_CALLS { return Err("工具调用数量过多".into()); }
                        self.calls.insert(index, Call {
                            id: block["id"].as_str().unwrap_or("").into(),
                            name: block["name"].as_str().unwrap_or("").into(), arguments: String::new(),
                        });
                    }
                    if block["type"] == "server_tool_use" { thought = Some("正在联网搜索…\n".into()); }
                    self.blocks.insert(index, block);
                }
                Some("content_block_delta") => {
                    let d = &v["delta"];
                    let block = self.blocks.get_mut(&index).ok_or("内容块顺序无效")?;
                    match d["type"].as_str() {
                        Some("text_delta") => { text = d["text"].as_str().map(str::to_owned); join_field(block, "text", &d["text"]); }
                        Some("thinking_delta") => { thought = d["thinking"].as_str().map(str::to_owned); join_field(block, "thinking", &d["thinking"]); }
                        Some("signature_delta") => join_field(block, "signature", &d["signature"]),
                        Some("input_json_delta") => {
                            // Server-side tools are continued verbatim, never dispatched locally.
                            if let Some(call) = self.calls.get_mut(&index) { append(&mut call.arguments, &d["partial_json"])?; }
                            else { join_field(block, "_partial_input", &d["partial_json"]); }
                        }
                        Some("citations_delta") => {
                            if !block["citations"].is_array() { block["citations"] = json!([]); }
                            block["citations"].as_array_mut().unwrap().push(d["citation"].clone());
                        }
                        _ => {}
                    }
                }
                Some("message_delta") => { self.finish = v["delta"]["stop_reason"].as_str().unwrap_or("").into(); }
                Some("message_stop") => self.complete = true,
                _ => {}
            }
        }
        if let Some(t) = &text { self.text.push_str(t); }
        if let Some(t) = &thought { self.reasoning.push_str(t); }
        Ok((text, thought))
    }
}

fn join_field(block: &mut Value, field: &str, part: &Value) {
    if let Some(part) = part.as_str() {
        let mut value = block[field].as_str().unwrap_or("").to_string();
        value.push_str(part); block[field] = Value::String(value);
    }
}

async fn stream_round(app: &AppHandle, response: reqwest::Response, anthropic: bool, target: Option<&str>) -> Result<Round, String> {
    if !response.status().is_success() { return Err(format!("{}: {}", response.status(), response.text().await.unwrap_or_default())); }
    let mut stream = response.bytes_stream();
    let mut pending = Vec::new();
    let mut round = Round::default();
    let mut total_bytes = 0usize;
    loop {
        let chunk = tokio::time::timeout(std::time::Duration::from_secs(120), stream.next()).await
            .map_err(|_| "AI响应超时；已执行的工具不会自动重试")?;
        let Some(chunk) = chunk else { break };
        let chunk = chunk.map_err(|e| e.to_string())?;
        total_bytes += chunk.len();
        if total_bytes > 8_000_000 { return Err("单轮响应过大，已停止接收".into()); }
        pending.extend_from_slice(&chunk);
        if pending.len() > 2_000_000 { return Err("响应行过大".into()); }
        while let Some(end) = pending.iter().position(|b| *b == b'\n') {
            let bytes: Vec<_> = pending.drain(..=end).collect();
            let line = std::str::from_utf8(&bytes).map_err(|_| "响应不是有效UTF-8")?.trim();
            let Some(data) = line.strip_prefix("data:").map(str::trim) else { continue };
            if data == "[DONE]" { round.complete = true; break; }
            if data.is_empty() { continue; }
            let event: Value = serde_json::from_str(data).map_err(|e| format!("响应事件无效：{e}"))?;
            let (text, thought) = round.event(&event, anthropic)?;
            if let Some(t) = text { let _ = emit_stream(app,target,"stream-chunk",t); }
            if let Some(t) = thought { let _ = emit_stream(app,target,"stream-reasoning",t); }
        }
        if round.complete { break; }
    }
    if !round.complete { return Err("AI响应中断，未执行未完成的工具调用".into()); }
    if ["length", "max_tokens"].contains(&round.finish.as_str()) { return Err("AI回复达到长度上限，未执行截断的工具调用".into()); }
    Ok(round)
}

fn emit_stream(app:&AppHandle,target:Option<&str>,event:&str,text:String)->tauri::Result<()>{
    if target==Some(""){Ok(())}else if let Some(target)=target{app.emit_to(target,event,text)}else{app.emit(event,text)}
}
pub async fn run(app:&AppHandle,client:&reqwest::Client,key:&str,cfg:&Config,system:&str,messages:Vec<Value>,registry:Vec<ai_tools::ToolSpec>)->Result<Answer,String>{
    run_scoped(app,client,key,cfg,system,messages,registry,None).await
}
pub async fn chat_in_window(app:&AppHandle,client:&reqwest::Client,key:&str,cfg:&Config,system:&str,messages:Vec<Value>)->Result<Answer,String>{
    run_scoped(app,client,key,cfg,system,messages,vec![],Some("")).await
}
async fn run_scoped(app: &AppHandle, client: &reqwest::Client, key: &str, cfg: &Config, system: &str, mut messages: Vec<Value>, registry: Vec<ai_tools::ToolSpec>, target:Option<&str>) -> Result<Answer, String> {
    let anthropic = cfg.ai.web_search;
    let system = format!("{}\n\n{}", ai_tools::POLICY, system);
    let allowed: std::collections::HashSet<_> = registry.iter().map(|s|s.name.clone()).collect();
    let tools: Vec<Value> = registry.iter().map(|s| if anthropic {
        json!({"name":s.name,"description":s.description,"input_schema":s.parameters})
    } else {
        json!({"type":"function","function":{"name":s.name,"description":s.description,"parameters":s.parameters}})
    }).collect();
    if !anthropic { messages.insert(0, json!({"role":"system","content":system})); }
    let history_len = messages.len();
    let mut final_text = String::new();
    let mut receipts: BTreeMap<String, (String, Value, Value)> = BTreeMap::new();
    let mut calls_count = 0;
    let result = async {
    for _ in 0..MAX_ROUNDS {
        let mut body = json!({"model":cfg.model(),"messages":messages,"stream":true,"tools":tools});
        let request = if anthropic {
            let (tokens, searches, budget) = cfg.search_limits();
            body["system"] = json!(system); body["max_tokens"] = json!(tokens);
            body["tools"].as_array_mut().unwrap().push(json!({"type":"web_search_20250305","name":"web_search","max_uses":searches}));
            if cfg.ai.deep_thinking { body["thinking"] = json!({"type":"enabled","budget_tokens":budget}); }
            client.post(format!("{}/anthropic/v1/messages", cfg.base_url())).header("x-api-key", key).header("anthropic-version", "2023-06-01")
        } else {
            body["thinking"] = json!({"type":if cfg.ai.deep_thinking {"enabled"} else {"disabled"}});
            body["tool_choice"] = json!("auto");
            client.post(format!("{}/chat/completions", cfg.base_url())).bearer_auth(key)
        };
        if !anthropic && tools.is_empty(){body.as_object_mut().unwrap().remove("tools");body.as_object_mut().unwrap().remove("tool_choice");}
        let response = tokio::time::timeout(std::time::Duration::from_secs(120), request.json(&body).send()).await
            .map_err(|_| "AI请求超时")?.map_err(|e| crate::describe_err(&e))?;
        let mut round = stream_round(app, response, anthropic,target).await?;
        final_text.push_str(&round.text);
        if round.calls.is_empty() && round.finish != "pause_turn" {
            if anthropic {
                normalize_blocks(&mut round.blocks)?;
                messages.push(json!({"role":"assistant","content":round.blocks.values().collect::<Vec<_>>()}));
            } else {
                messages.push(json!({"role":"assistant","content":round.text,"reasoning_content":round.reasoning}));
            }
            return Ok(Answer { text: final_text, protocol: ProtocolTurn {
                provider: if anthropic {"anthropic"} else {"openai"}.into(),
                messages: messages[history_len..].to_vec(),
            }});
        }
        if !round.calls.is_empty() && round.finish != if anthropic { "tool_use" } else { "tool_calls" } {
            return Err("工具调用未正常结束，已停止执行".into());
        }
        calls_count += round.calls.len();
        if calls_count > MAX_CALLS { return Err("本轮工具调用达到上限，已停止继续执行".into()); }
        let mut parsed = Vec::new();
        // Validate the entire response envelope before running any operation.
        let mut ids = std::collections::HashSet::new();
        for (index, call) in &round.calls {
            if !allowed.contains(&call.name) {return Err("AI请求了本轮工具范围以外的能力，已停止执行".into());}
            if call.id.is_empty() || call.name.is_empty() || !ids.insert(call.id.clone()) { return Err("工具调用ID或名称无效".into()); }
            let args = if anthropic && call.arguments.is_empty() { round.blocks[index]["input"].clone() }
                else { serde_json::from_str(&call.arguments).map_err(|e| format!("工具参数JSON不完整：{e}"))? };
            if anthropic { round.blocks.get_mut(index).unwrap()["input"] = args.clone(); }
            parsed.push((call.clone(), args));
        }
        if anthropic {
            normalize_blocks(&mut round.blocks)?;
            messages.push(json!({"role":"assistant","content":round.blocks.values().collect::<Vec<_>>()}));
        } else {
            let mut assistant = json!({"role":"assistant","content":round.text,"tool_calls":parsed.iter().map(|(call,args)| json!({"id":call.id,"type":"function","function":{"name":call.name,"arguments":args.to_string()}})).collect::<Vec<_>>()});
            assistant["reasoning_content"] = json!(round.reasoning);
            messages.push(assistant);
        }
        let mut results = Vec::new();
        for (call, args) in parsed {
            let output = if let Some((name, old_args, output)) = receipts.get(&call.id) {
                if name != &call.name || old_args != &args { return Err("重复的调用ID对应不同参数，已停止".into()); }
                output.clone()
            } else {
                let output = ai_tools::dispatch(app, &call.name, args.clone()).await;
                receipts.insert(call.id.clone(), (call.name.clone(), args, output.clone()));
                output
            };
            if anthropic { results.push(json!({"type":"tool_result","tool_use_id":call.id,"content":output.to_string(),"is_error":output["ok"] != true})); }
            else { messages.push(json!({"role":"tool","tool_call_id":call.id,"content":output.to_string()})); }
        }
        if anthropic && !results.is_empty() { messages.push(json!({"role":"user","content":results})); }
    }
    Err("工具调用达到轮数上限。已有操作可能已完成，请查询状态后继续。".into())
    }.await;
    result.map_err(|error: String| {
        if receipts.is_empty() { return error; }
        let successful: Vec<_> = receipts.values().filter(|(_, _, result)| result["ok"] == true).map(|(name, _, _)| name.as_str()).collect();
        if successful.is_empty() { error }
        else { format!("{error}\n本轮已成功执行：{}。这些操作不会因回复失败而撤销，请先查询状态，不要重复提交。", successful.join("、")) }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn assembles_interleaved_tool_arguments() {
        let mut r = Round::default();
        r.event(&json!({"choices":[{"delta":{"tool_calls":[{"index":0,"id":"a","function":{"name":"notes_add","arguments":"{\"text\":"}},{"index":1,"id":"b","function":{"name":"notes_list","arguments":"{}"}}]}}]}), false).unwrap();
        r.event(&json!({"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"\"牛奶\"}"}}]},"finish_reason":"tool_calls"}]}), false).unwrap();
        assert_eq!(serde_json::from_str::<Value>(&r.calls[&0].arguments).unwrap()["text"], "牛奶");
        assert_eq!(r.calls[&1].name, "notes_list");
        assert_eq!(r.finish, "tool_calls");
    }
    #[test] fn anthropic_keeps_signed_thinking_and_tool_inputs() {
        let mut r = Round::default();
        for event in [
            json!({"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":"","signature":""}}),
            json!({"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"考虑"}}),
            json!({"type":"content_block_delta","index":0,"delta":{"type":"signature_delta","signature":"sig"}}),
            json!({"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"x","name":"notes_list","input":{}}}),
            json!({"type":"message_delta","delta":{"stop_reason":"tool_use"}}),
            json!({"type":"message_stop"}),
        ] { r.event(&event, true).unwrap(); }
        assert_eq!(r.blocks[&0]["signature"], "sig");
        assert_eq!(r.calls[&1].name, "notes_list");
        assert!(r.complete);
    }
    #[test] fn rejects_oversized_arguments_and_errors() {
        let mut r = Round::default();
        assert!(r.event(&json!({"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"x".repeat(MAX_ARGUMENT_BYTES + 1)}}]}}]}), false).is_err());
        assert!(Round::default().event(&json!({"error":{"message":"failed"}}), true).is_err());
    }
    #[test] fn history_keeps_complete_protocol_turns_and_switches_provider_safely() {
        let raw = vec![json!({"role":"assistant","content":"","reasoning_content":"think","tool_calls":[{"id":"a"}]}), json!({"role":"tool","tool_call_id":"a","content":"ok"}), json!({"role":"assistant","content":"done","reasoning_content":"finished"})];
        let messages = vec![crate::ChatMessage { role:"assistant".into(), content:"done".into(), protocol:Some(ProtocolTurn {provider:"openai".into(),messages:raw.clone()}) }];
        assert_eq!(history(&messages, false), raw);
        assert_eq!(history(&messages, true), vec![json!({"role":"assistant","content":"done"})]);
    }
}
