use super::*;

pub(super) const DEFAULT_API_RATE_LIMIT_PER_SEC: u32 = 180;

#[derive(Clone)]
pub(super) struct ApiSecurity {
    pub required_token: Option<String>,
    pub rate_limit_per_sec: u32,
    pub buckets: Arc<Mutex<HashMap<String, RateBucket>>>,
}

#[derive(Clone)]
pub(super) struct RateBucket {
    pub window_start: std::time::Instant,
    pub count: u32,
}

impl ApiSecurity {
    pub(super) fn from_env() -> Self {
        let required_token = std::env::var("AXIOM_API_TOKEN")
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty());
        let rate_limit_per_sec = std::env::var("AXIOM_API_RATE_LIMIT_PER_SEC")
            .ok()
            .and_then(|v| v.parse::<u32>().ok())
            .unwrap_or(DEFAULT_API_RATE_LIMIT_PER_SEC)
            .max(1);
        Self {
            required_token,
            rate_limit_per_sec,
            buckets: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

pub(super) async fn api_guard(
    State(security): State<ApiSecurity>,
    req: Request,
    next: Next,
) -> axum::response::Response {
    if let Some(expected_token) = security.required_token.as_deref() {
        let auth_header = req
            .headers()
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .map(str::trim)
            .unwrap_or("");
        let api_key_header = req
            .headers()
            .get("x-api-key")
            .and_then(|v| v.to_str().ok())
            .map(str::trim)
            .unwrap_or("");

        let bearer = auth_header
            .strip_prefix("Bearer ")
            .or_else(|| auth_header.strip_prefix("bearer "))
            .unwrap_or(auth_header);

        if bearer != expected_token && api_key_header != expected_token {
            return (
                StatusCode::UNAUTHORIZED,
                Json(ApiResponse::err(
                    "Unauthorized: set AXIOM_API_TOKEN and send Authorization: Bearer <token>",
                )),
            )
                .into_response();
        }
    }

    let key = req
        .headers()
        .get("x-forwarded-for")
        .or_else(|| req.headers().get("x-real-ip"))
        .and_then(|v| v.to_str().ok())
        .unwrap_or("local")
        .to_string();

    {
        let mut buckets = security.buckets.lock().unwrap();
        let now = std::time::Instant::now();
        let entry = buckets.entry(key).or_insert(RateBucket {
            window_start: now,
            count: 0,
        });
        if now.duration_since(entry.window_start).as_secs_f32() >= 1.0 {
            entry.window_start = now;
            entry.count = 0;
        }
        entry.count = entry.count.saturating_add(1);
        if entry.count > security.rate_limit_per_sec {
            return (
                StatusCode::TOO_MANY_REQUESTS,
                Json(ApiResponse::err("Rate limit exceeded")),
            )
                .into_response();
        }

        if buckets.len() > 4096 {
            buckets.retain(|_, v| now.duration_since(v.window_start).as_secs_f32() < 10.0);
        }
    }

    next.run(req).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{http::Request as HttpRequest, routing::get, Router};
    use tower::util::ServiceExt;

    async fn ok_handler() -> &'static str {
        "ok"
    }

    #[tokio::test]
    async fn rejects_when_token_missing_or_invalid() {
        let security = ApiSecurity {
            required_token: Some("secret".to_string()),
            rate_limit_per_sec: 100,
            buckets: Arc::new(Mutex::new(HashMap::new())),
        };
        let app = Router::new()
            .route("/", get(ok_handler))
            .layer(middleware::from_fn_with_state(security, api_guard));

        let req = HttpRequest::builder()
            .uri("/")
            .body(axum::body::Body::empty())
            .expect("request");
        let res = app.clone().oneshot(req).await.expect("response");
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);

        let req_bad = HttpRequest::builder()
            .uri("/")
            .header("authorization", "Bearer nope")
            .body(axum::body::Body::empty())
            .expect("request");
        let res_bad = app.oneshot(req_bad).await.expect("response");
        assert_eq!(res_bad.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn allows_valid_token_and_applies_rate_limit() {
        let security = ApiSecurity {
            required_token: Some("secret".to_string()),
            rate_limit_per_sec: 1,
            buckets: Arc::new(Mutex::new(HashMap::new())),
        };
        let app = Router::new()
            .route("/", get(ok_handler))
            .layer(middleware::from_fn_with_state(security, api_guard));

        let req_ok = HttpRequest::builder()
            .uri("/")
            .header("authorization", "Bearer secret")
            .header("x-real-ip", "127.0.0.1")
            .body(axum::body::Body::empty())
            .expect("request");
        let res_ok = app.clone().oneshot(req_ok).await.expect("response");
        assert_eq!(res_ok.status(), StatusCode::OK);

        let req_limited = HttpRequest::builder()
            .uri("/")
            .header("authorization", "Bearer secret")
            .header("x-real-ip", "127.0.0.1")
            .body(axum::body::Body::empty())
            .expect("request");
        let res_limited = app.oneshot(req_limited).await.expect("response");
        assert_eq!(res_limited.status(), StatusCode::TOO_MANY_REQUESTS);
    }
}
