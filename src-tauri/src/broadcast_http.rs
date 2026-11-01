use std::{collections::BTreeMap, sync::Arc, time::Duration};
use tauri::AppHandle;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::{watch, Semaphore},
    task::JoinSet,
};
#[derive(Debug, PartialEq)]
enum Route {
    Page,
    Style,
    Script,
    Posts,
}
fn route(raw: &str, host: &str, code: &str) -> Result<Route, u16> {
    let mut lines = raw.split("\r\n");
    let first: Vec<_> = lines.next().ok_or(400u16)?.split(' ').collect();
    if first.len() != 3 || !["HTTP/1.0", "HTTP/1.1"].contains(&first[2]) {
        return Err(400);
    }
    if first[0] != "GET" {
        return Err(405);
    }
    let mut headers = BTreeMap::new();
    for line in lines {
        if line.is_empty() {
            break;
        }
        let (key, value) = line.split_once(':').ok_or(400u16)?;
        if key.is_empty() || !key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') {
            return Err(400);
        }
        if headers
            .insert(key.to_ascii_lowercase(), value.trim())
            .is_some()
        {
            return Err(400);
        }
    }
    if headers.get("host").copied() != Some(host) {
        return Err(403);
    }
    if headers
        .get("origin")
        .map(|o| *o != format!("http://{host}"))
        .unwrap_or(false)
    {
        return Err(403);
    }
    if headers.contains_key("transfer-encoding")
        || headers
            .get("content-length")
            .map(|n| *n != "0")
            .unwrap_or(false)
    {
        return Err(400);
    }
    match first[1] {
        "/" => Ok(Route::Page),
        "/board.css" => Ok(Route::Style),
        "/board.js" => Ok(Route::Script),
        "/api/posts" => {
            let auth = headers
                .get("authorization")
                .and_then(|s| s.strip_prefix("Bearer "))
                .unwrap_or("");
            let mut diff = auth.len() ^ code.len();
            for (a, b) in auth.bytes().zip(code.bytes()) {
                diff |= (a ^ b) as usize;
            }
            if diff != 0 {
                return Err(401);
            }
            Ok(Route::Posts)
        }
        _ => Err(404),
    }
}
fn response(status: u16, mime: &str, body: &str) -> Vec<u8> {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        _ => "Unavailable",
    };
    let head=format!("HTTP/1.1 {status} {reason}\r\nContent-Type: {mime}; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nReferrer-Policy: no-referrer\r\nContent-Security-Policy: default-src 'none'; script-src 'self'; style-src 'self'; connect-src 'self'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'\r\n\r\n",body.len());
    [head.as_bytes(), body.as_bytes()].concat()
}
type Source = Arc<dyn Fn() -> Result<Vec<crate::broadcast::Post>, String> + Send + Sync>;
async fn handle(mut stream: TcpStream, source: Source, host: String, code: String) {
    let mut bytes = Vec::new();
    let mut chunk = [0u8; 1024];
    let route = loop {
        let Ok(n) = stream.read(&mut chunk).await else {
            return;
        };
        if n == 0 {
            return;
        }
        bytes.extend_from_slice(&chunk[..n]);
        if bytes.len() > 4096 {
            break Err(400);
        }
        if bytes.windows(4).any(|w| w == b"\r\n\r\n") {
            break std::str::from_utf8(&bytes)
                .map_err(|_| 400u16)
                .and_then(|s| route(s, &host, &code));
        }
    };
    let (status, mime, body) = match route {
        Ok(Route::Page) => (
            200,
            "text/html",
            include_str!("../../src/broadcast-viewer.html").to_owned(),
        ),
        Ok(Route::Style) => (
            200,
            "text/css",
            include_str!("../../src/broadcast-viewer.css").to_owned(),
        ),
        Ok(Route::Script) => (
            200,
            "application/javascript",
            include_str!("../../src/broadcast-viewer.js").to_owned(),
        ),
        Ok(Route::Posts) => match source() {
            Ok(posts) => (
                200,
                "application/json",
                serde_json::json!({"posts":posts}).to_string(),
            ),
            Err(_) => (503, "text/plain", "暂时无法读取广播".into()),
        },
        Err(code) => (code, "text/plain", "请求无效或访问码已失效".into()),
    };
    let _ = stream.write_all(&response(status, mime, &body)).await;
}
pub async fn serve(
    listener: TcpListener,
    app: AppHandle,
    host: String,
    code: String,
    stop: watch::Receiver<bool>,
) {
    serve_core(
        listener,
        Arc::new(move || crate::broadcast::visible(&app)),
        host,
        code,
        stop,
    )
    .await;
}
async fn serve_core(
    listener: TcpListener,
    source: Source,
    host: String,
    code: String,
    mut stop: watch::Receiver<bool>,
) {
    let limit = Arc::new(Semaphore::new(32));
    let mut tasks = JoinSet::new();
    loop {
        tokio::select! {biased;_=stop.changed()=>break,Some(_)=tasks.join_next(),if !tasks.is_empty()=>{},accepted=listener.accept()=>{let Ok((stream,peer))=accepted else{break;};if !matches!(peer.ip(),std::net::IpAddr::V4(ip)if ip.is_private()||ip.is_loopback()){continue;}let Ok(permit)=limit.clone().try_acquire_owned()else{continue;};let(source,host,code)=(source.clone(),host.clone(),code.clone());tasks.spawn(async move{let _permit=permit;let _=tokio::time::timeout(Duration::from_secs(8),handle(stream,source,host,code)).await;});}}
    }
    tasks.abort_all();
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn loopback_server_serves_only_authorized_public_posts_and_stops() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let host = listener.local_addr().unwrap().to_string();
        let (tx, rx) = watch::channel(false);
        let source: Source = Arc::new(|| {
            Ok(vec![crate::broadcast::Post {
                id: 1,
                title: "测试通知".into(),
                content: "公开内容<script>literal</script>".into(),
                pinned: false,
                created_at: 1,
                expires_at: u64::MAX,
                withdrawn: false,
            }])
        });
        let task = tokio::spawn(serve_core(
            listener,
            source,
            host.clone(),
            "test-code".into(),
            rx,
        ));
        async fn fetch(host: &str, auth: &str) -> String {
            let mut stream = TcpStream::connect(host).await.unwrap();
            stream
                .write_all(
                    format!("GET /api/posts HTTP/1.1\r\nHost: {host}\r\n{auth}\r\n").as_bytes(),
                )
                .await
                .unwrap();
            let mut out = Vec::new();
            tokio::time::timeout(Duration::from_secs(2), stream.read_to_end(&mut out))
                .await
                .unwrap()
                .unwrap();
            String::from_utf8(out).unwrap()
        }
        let denied = fetch(&host, "").await;
        assert!(denied.starts_with("HTTP/1.1 401"));
        assert!(!denied.contains("公开内容"));
        let allowed = fetch(&host, "Authorization: Bearer test-code\r\n").await;
        assert!(allowed.starts_with("HTTP/1.1 200"));
        let body: serde_json::Value =
            serde_json::from_str(allowed.split_once("\r\n\r\n").unwrap().1).unwrap();
        assert_eq!(body["posts"][0]["title"], "测试通知");
        tx.send(true).unwrap();
        tokio::time::timeout(Duration::from_secs(2), task)
            .await
            .unwrap()
            .unwrap();
        assert!(TcpStream::connect(&host).await.is_err());
    }
    #[test]
    fn rejects_unauthorized_hosts_origins_write_methods_and_smuggling() {
        let host = "192.168.1.2:1234";
        let req =
            |path: &str, extra: &str| format!("GET {path} HTTP/1.1\r\nHost: {host}\r\n{extra}\r\n");
        assert_eq!(route(&req("/", ""), host, "secret"), Ok(Route::Page));
        assert_eq!(route(&req("/api/posts", ""), host, "secret"), Err(401));
        assert_eq!(
            route(
                &req("/api/posts", "Authorization: Bearer secret\r\n"),
                host,
                "secret"
            ),
            Ok(Route::Posts)
        );
        for extra in [
            "Origin: https://evil.test\r\n",
            "Host: evil.test\r\n",
            "Content-Length: 2\r\n",
            "Transfer-Encoding: chunked\r\n",
        ] {
            assert!(route(&req("/api/posts", extra), host, "secret").is_err());
        }
        assert_eq!(
            route(&req("/../../wechat-chats.json", ""), host, "secret"),
            Err(404)
        );
        assert_eq!(
            route("POST / HTTP/1.1\r\nHost: x\r\n\r\n", host, "secret"),
            Err(405)
        );
        assert!(route(
            &req("/api/posts", "Authorization: Bearer secre\r\n"),
            host,
            "secret"
        )
        .is_err());
        assert!(String::from_utf8(response(200, "text/html", "你好"))
            .unwrap()
            .contains("Content-Length: 6"));
    }
}
