//! Optional same-origin Flutter web host. Native clients retain their TLS listener.
use super::*;
use axum::{
    extract::Request,
    middleware::{self, Next},
};
use std::collections::HashSet;

const COOKIE: &str = "openfader_web_session";

pub(super) fn resolve_dir(explicit: Option<PathBuf>) -> Option<PathBuf> {
    if let Some(dir) = explicit {
        return Some(dir);
    }
    if let Some(dir) = std::env::var_os("OPENFADER_WEB_DIR") {
        return Some(dir.into());
    }
    let mut candidates = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            candidates.push(parent.join("../Resources/web"));
            candidates.push(parent.join("web"));
        }
    }
    candidates.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../app/build/web"));
    candidates.into_iter().find(|dir| assets_available(dir))
}
fn assets_available(dir: &StdPath) -> bool {
    dir.join("index.html").is_file() && dir.join("flutter_bootstrap.js").is_file()
}
pub(super) fn token(headers: &axum::http::HeaderMap) -> Option<&str> {
    headers
        .get(header::COOKIE)?
        .to_str()
        .ok()?
        .split(';')
        .find_map(|entry| {
            let (key, value) = entry.trim().split_once('=')?;
            (key == COOKIE && !value.is_empty()).then_some(value)
        })
}
pub(super) fn authorized_token<'a>(
    app: &AppState,
    headers: &'a axum::http::HeaderMap,
) -> Option<&'a str> {
    let token = token(headers)?;
    let id = app.remote.access.authenticate(token)?;
    app.remote
        .web_clients
        .lock()
        .unwrap()
        .contains(&id)
        .then_some(token)
}
fn same_origin(headers: &axum::http::HeaderMap) -> bool {
    let Some(host) = headers.get(header::HOST).and_then(|h| h.to_str().ok()) else {
        return false;
    };
    let Some(origin) = headers
        .get(header::ORIGIN)
        .and_then(|h| h.to_str().ok())
        .and_then(|v| v.parse::<axum::http::Uri>().ok())
    else {
        return false;
    };
    origin.scheme_str() == Some("http")
        && origin
            .authority()
            .is_some_and(|a| a.as_str().eq_ignore_ascii_case(host))
        && origin.path_and_query().is_none_or(|p| p.as_str() == "/")
}
async fn origin_guard(
    AxumState(app): AxumState<AppState>,
    request: Request,
    next: Next,
) -> axum::response::Response {
    let needs_origin = !matches!(
        *request.method(),
        axum::http::Method::GET | axum::http::Method::HEAD
    ) || request.uri().path() == "/ws";
    if needs_origin && !same_origin(request.headers()) {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({"error":"same-origin browser request required"})),
        )
            .into_response();
    }
    if request.uri().path().starts_with("/api/")
        && authorized_token(&app, request.headers()).is_none()
    {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error":"Browser login required"})),
        )
            .into_response();
    }
    let sensitive =
        request.uri().path().starts_with("/api/") || request.uri().path().starts_with("/web/");
    let mut response = next.run(request).await;
    if sensitive {
        response.headers_mut().insert(
            header::CACHE_CONTROL,
            axum::http::HeaderValue::from_static("no-store"),
        );
    }
    response
}
async fn connect_info(AxumState(app): AxumState<AppState>) -> Json<serde_json::Value> {
    Json(serde_json::json!({"password_required":app.remote.access.password_required()}))
}
async fn connect(
    AxumState(app): AxumState<AppState>,
    Json(body): Json<serde_json::Value>,
) -> axum::response::Response {
    let password = body["password"].as_str().unwrap_or("").to_owned();
    let remote = app.remote.clone();
    let result = tokio::task::spawn_blocking(move || remote.access.connect(&password)).await;
    match result {
        Ok(Ok(client)) => {
            app.remote
                .web_clients
                .lock()
                .unwrap()
                .insert(client.client_id.clone());
            (
                [(
                    header::SET_COOKIE,
                    format!(
                        "{COOKIE}={}; Path=/; HttpOnly; SameSite=Strict; Max-Age=43200",
                        client.bearer_token
                    ),
                )],
                Json(serde_json::json!({"client_id":client.client_id})),
            )
                .into_response()
        }
        Ok(Err(e)) if e.to_string() == "retry later" => (
            StatusCode::TOO_MANY_REQUESTS,
            Json(serde_json::json!({"error":"Please wait before trying again"})),
        )
            .into_response(),
        _ => (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error":"Invalid server password or server disabled"})),
        )
            .into_response(),
    }
}
async fn disconnect(
    AxumState(app): AxumState<AppState>,
    headers: axum::http::HeaderMap,
) -> impl IntoResponse {
    if let Some(token) = authorized_token(&app, &headers) {
        if let Some(id) = app.remote.access.authenticate(token) {
            app.remote.web_clients.lock().unwrap().remove(&id);
            app.remote.access.revoke(&id);
        }
    }
    (
        [(
            header::SET_COOKIE,
            format!("{COOKIE}=; Path=/; HttpOnly; SameSite=Strict; Max-Age=0"),
        )],
        Json(serde_json::json!({"ok":true})),
    )
}
async fn root(AxumState(app): AxumState<AppState>) -> axum::response::Response {
    asset(&app, "index.html").await
}
async fn file(
    AxumState(app): AxumState<AppState>,
    Path(path): Path<String>,
) -> axum::response::Response {
    asset(&app, &path).await
}
async fn asset(app: &AppState, path: &str) -> axum::response::Response {
    let Some(dir) = app.static_dir.as_deref() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if path.split('/').any(|part| part == ".." || part == ".") || path.contains('\\') {
        return StatusCode::NOT_FOUND.into_response();
    }
    let Ok(root) = tokio::fs::canonicalize(dir).await else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let requested = root.join(path);
    let path = match tokio::fs::canonicalize(&requested).await {
        Ok(path) if path.starts_with(&root) && path.is_file() => path,
        Ok(_) => return StatusCode::NOT_FOUND.into_response(),
        Err(_) if StdPath::new(path).extension().is_none() && !path.starts_with("api/") => {
            root.join("index.html")
        }
        Err(_) => return StatusCode::NOT_FOUND.into_response(),
    };
    match tokio::fs::read(&path).await {
        Ok(bytes) => (
            [
                (
                    header::CONTENT_TYPE,
                    content_type(path.to_str().unwrap_or("")),
                ),
                (header::CACHE_CONTROL, "no-cache"),
                (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
            ],
            bytes,
        )
            .into_response(),
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}
fn router(app: AppState) -> Router {
    Router::new()
        .route("/web/connect", get(connect_info).post(connect))
        .route("/web/disconnect", axum::routing::post(disconnect))
        .route("/ws", get(ws_handler))
        .route("/api/recorder/v1/state", get(recorder_state))
        .route("/api/recorder/v1/devices", get(recorder_devices))
        .route("/api/recorder/v1/takes", get(recorder_takes))
        .route(
            "/api/recorder/v1/devices/refresh",
            axum::routing::post(recorder_refresh_devices),
        )
        .route(
            "/api/recorder/v1/inspect",
            axum::routing::post(recorder_inspect),
        )
        .route(
            "/api/recorder/v1/waveform",
            axum::routing::post(recorder_waveform),
        )
        .route(
            "/api/recorder/v1/command",
            axum::routing::post(recorder_command),
        )
        .route("/", get(root))
        .route("/{*path}", get(file))
        .layer(middleware::from_fn_with_state(app.clone(), origin_guard))
        .with_state(app)
}
pub(super) async fn status(app: &AppState) -> serde_json::Value {
    let listener = app.remote.web_listener.lock().await;
    let available = app
        .remote
        .web_dir
        .as_ref()
        .is_some_and(|dir| assets_available(dir));
    let urls: Vec<String> = listener
        .as_ref()
        .map(|(addr, _, _)| {
            if addr.ip().is_unspecified() {
                openfader_transport::discovery::local_ipv4_interfaces()
                    .into_iter()
                    .filter(|i| !i.ip.is_loopback())
                    .map(|i| format!("http://{}:{}/", i.ip, addr.port()))
                    .collect()
            } else {
                vec![format!("http://{addr}/")]
            }
        })
        .unwrap_or_default();
    serde_json::json!({"web_enabled":listener.is_some(),"web_port":listener.as_ref().map(|(a,_,_)|a.port()),"web_bind":listener.as_ref().map(|(a,_,_)|a.ip().to_string()),"web_urls":urls,"web_assets_available":available,"web_reason":if available {None} else {Some("Flutter web assets are unavailable. Build and bundle the web app before enabling browser hosting.")}})
}
pub(super) async fn enable(app: &AppState, ip: std::net::IpAddr, port: u16) -> Result<(), String> {
    let Some(dir) = app
        .remote
        .web_dir
        .as_ref()
        .filter(|dir| assets_available(dir))
    else {
        return Err(
            "Flutter web assets are unavailable. Build and bundle the web app first.".into(),
        );
    };
    let mut listener = app.remote.web_listener.lock().await;
    if let Some((addr, _, _)) = &*listener {
        return if addr.port() == port && addr.ip() == ip {
            Ok(())
        } else {
            Err("Disable browser hosting before changing its address or port".into())
        };
    }
    let bound = tokio::net::TcpListener::bind((ip, port))
        .await
        .map_err(|e| format!("Could not bind browser port: {e}"))?;
    let addr = bound.local_addr().map_err(|e| e.to_string())?;
    let (stop, stopped) = tokio::sync::oneshot::channel();
    let mut state = app.clone();
    state.remote_audience = true;
    state.web_audience = true;
    state.static_dir = Some(Arc::new(dir.clone()));
    let router = router(state);
    app.remote.access.set_enabled(true);
    let task = tokio::spawn(async move {
        let _ = axum::serve(bound, router)
            .with_graceful_shutdown(async {
                let _ = stopped.await;
            })
            .await;
    });
    *listener = Some((addr, stop, task));
    Ok(())
}
pub(super) async fn disable(app: &AppState, native_enabled: bool) {
    let clients: HashSet<_> = std::mem::take(&mut *app.remote.web_clients.lock().unwrap());
    for id in clients {
        app.remote.access.revoke(&id);
    }
    if let Some((_, stop, task)) = app.remote.web_listener.lock().await.take() {
        let _ = stop.send(());
        let _ = task.await;
    }
    if !native_enabled {
        app.remote.access.set_enabled(false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;

    fn fixture(label: &str) -> (AppState, PathBuf) {
        let root = std::env::temp_dir().join(format!(
            "openfader-web-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("index.html"), "flutter fixture index").unwrap();
        std::fs::write(root.join("flutter_bootstrap.js"), "fixtureBootstrap();").unwrap();
        let remote = Arc::new(RemoteHost {
            access: remote_access::RemoteAccess::new(&MemoryIdentity::default())
                .unwrap()
                .with_password_file(root.join("password.json"))
                .unwrap(),
            listener: Mutex::new(None),
            web_listener: Mutex::new(None),
            web_clients: std::sync::Mutex::new(HashSet::new()),
            web_dir: Some(root.clone()),
            microphone_authorized: std::sync::atomic::AtomicBool::new(false),
        });
        (
            AppState {
                engine: openfader_engine::Engine::new(Default::default()),
                static_dir: None,
                sim_only: true,
                recorder: None,
                recorder_token: None,
                desktop_instance: None,
                shutdown: Arc::new(Notify::new()),
                remote,
                remote_audience: false,
                web_audience: false,
            },
            root,
        )
    }
    async fn request(
        addr: std::net::SocketAddr,
        method: &str,
        path: &str,
        extra: &str,
        body: &str,
    ) -> String {
        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        let data=format!("{method} {path} HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n{extra}Content-Length: {}\r\n\r\n{body}",body.len());
        stream.write_all(data.as_bytes()).await.unwrap();
        let mut bytes = Vec::new();
        stream.read_to_end(&mut bytes).await.unwrap();
        String::from_utf8(bytes).unwrap()
    }
    fn cookie(response: &str) -> String {
        response
            .lines()
            .find_map(|line| line.strip_prefix("set-cookie: "))
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_owned()
    }

    #[tokio::test]
    async fn browser_cookie_origin_and_static_boundaries() {
        let (app, root) = fixture("cookie");
        enable(&app, "127.0.0.1".parse().unwrap(), 0).await.unwrap();
        let addr = app.remote.web_listener.lock().await.as_ref().unwrap().0;
        assert!(request(addr, "GET", "/api/recorder/v1/state", "", "")
            .await
            .starts_with("HTTP/1.1 401"));
        let index = request(addr, "GET", "/", "", "").await;
        assert!(index.starts_with("HTTP/1.1 200"));
        assert!(index.contains("flutter fixture index"));
        let missing = request(addr, "GET", "/missing.js", "", "").await;
        assert!(missing.starts_with("HTTP/1.1 404"));
        let bad = request(
            addr,
            "POST",
            "/web/connect",
            "Origin: http://other.invalid\r\nContent-Type: application/json\r\n",
            "{}",
        )
        .await;
        assert!(bad.starts_with("HTTP/1.1 403"));
        let origin = format!("Origin: http://{addr}\r\nContent-Type: application/json\r\n");
        let login = request(addr, "POST", "/web/connect", &origin, "{}").await;
        assert!(login.starts_with("HTTP/1.1 200"));
        assert!(login.contains("HttpOnly; SameSite=Strict"));
        let cookie = cookie(&login);
        let body = login.split("\r\n\r\n").nth(1).unwrap();
        assert!(!body.contains("token"));
        let mut ws = format!("ws://{addr}/ws").into_client_request().unwrap();
        ws.headers_mut()
            .insert(header::ORIGIN, format!("http://{addr}").parse().unwrap());
        ws.headers_mut()
            .insert(header::COOKIE, cookie.parse().unwrap());
        let (socket, _) = tokio_tungstenite::connect_async(ws).await.unwrap();
        drop(socket);
        let mut bad_ws = format!("ws://{addr}/ws").into_client_request().unwrap();
        bad_ws
            .headers_mut()
            .insert(header::ORIGIN, "http://other.invalid".parse().unwrap());
        bad_ws
            .headers_mut()
            .insert(header::COOKIE, cookie.parse().unwrap());
        assert!(tokio_tungstenite::connect_async(bad_ws).await.is_err());
        let logout = request(
            addr,
            "POST",
            "/web/disconnect",
            &format!("{origin}Cookie: {cookie}\r\n"),
            "{}",
        )
        .await;
        assert!(logout.starts_with("HTTP/1.1 200"));
        let mut headers = axum::http::HeaderMap::new();
        headers.insert(header::COOKIE, cookie.parse().unwrap());
        assert!(authorized_token(&app, &headers).is_none());
        #[cfg(unix)]
        {
            let outside = root.with_extension("secret");
            std::fs::write(&outside, "private outside").unwrap();
            std::os::unix::fs::symlink(&outside, root.join("outside.txt")).unwrap();
            assert!(request(addr, "GET", "/outside.txt", "", "")
                .await
                .starts_with("HTTP/1.1 404"));
            std::fs::remove_file(outside).unwrap();
        }
        disable(&app, false).await;
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn missing_flutter_assets_do_not_claim_hosting_success() {
        let (app, root) = fixture("missing");
        std::fs::remove_file(root.join("flutter_bootstrap.js")).unwrap();
        assert!(enable(&app, "127.0.0.1".parse().unwrap(), 0).await.is_err());
        let status = status(&app).await;
        assert_eq!(status["web_enabled"], false);
        assert_eq!(status["web_assets_available"], false);
        assert!(status["web_reason"]
            .as_str()
            .is_some_and(|value| !value.is_empty()));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn shared_password_and_independent_native_sessions() {
        let (app, root) = fixture("password");
        app.remote
            .access
            .set_password("web-fixture-password")
            .unwrap();
        enable(&app, "127.0.0.1".parse().unwrap(), 0).await.unwrap();
        let addr = app.remote.web_listener.lock().await.as_ref().unwrap().0;
        assert!(request(addr, "GET", "/web/connect", "", "")
            .await
            .contains("\"password_required\":true"));
        let origin = format!("Origin: http://{addr}\r\nContent-Type: application/json\r\n");
        assert!(request(addr, "POST", "/web/connect", &origin, "{}")
            .await
            .starts_with("HTTP/1.1 401"));
        tokio::time::sleep(Duration::from_millis(1100)).await;
        assert!(request(
            addr,
            "POST",
            "/web/connect",
            &origin,
            r#"{"password":"wrong"}"#
        )
        .await
        .starts_with("HTTP/1.1 401"));
        tokio::time::sleep(Duration::from_millis(1100)).await;
        let login = request(
            addr,
            "POST",
            "/web/connect",
            &origin,
            r#"{"password":"web-fixture-password"}"#,
        )
        .await;
        assert!(login.starts_with("HTTP/1.1 200"));
        let cookie = cookie(&login);
        let native = app.remote.access.connect("web-fixture-password").unwrap();
        let mut headers = axum::http::HeaderMap::new();
        headers.insert(header::COOKIE, cookie.parse().unwrap());
        assert!(authorized_token(&app, &headers).is_some());
        disable(&app, true).await;
        assert!(authorized_token(&app, &headers).is_none());
        assert!(app
            .remote
            .access
            .authenticate(&native.bearer_token)
            .is_some());
        app.remote.access.set_enabled(false);
        std::fs::remove_dir_all(root).unwrap();
    }
}
