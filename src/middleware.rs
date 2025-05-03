use crate::errors::ServerError;
use anyhow::{Result, anyhow};
use axum::{
    body::HttpBody,
    extract::Request,
    http::{HeaderMap, StatusCode, header::CONTENT_TYPE},
    middleware::Next,
    response::Response,
};

pub async fn validate_body_length(request: Request, next: Next) -> Result<Response, ServerError> {
    let (parts, body) = request.into_parts();
    let size_hint = body.size_hint();
    if size_hint.lower() > 1024 {
        return Ok(Response::builder()
            .status(StatusCode::PAYLOAD_TOO_LARGE)
            .body("Request body too large".into())
            .map_err(|e| anyhow!("could not create response: {}", e))?);
    }

    Ok(next.run(Request::from_parts(parts, body)).await)
}

pub async fn validate_content_type(
    headers: HeaderMap,
    request: Request,
    next: Next,
) -> Result<Response, ServerError> {
    let content_type = match headers.get(CONTENT_TYPE) {
        Some(ct) => ct,
        None => {
            return Ok(Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body("Missing Content-Type header".into())
                .map_err(|e| anyhow!("could not create response: {}", e))?);
        }
    };

    let content_type = content_type
        .to_str()
        .map_err(|e| anyhow!("could not convert Content-Type header to string: {}", e))?;

    if !matches!(content_type, "application/json" | "text/plain") {
        return Ok(Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body("Invalid Content-Type header".into())
            .map_err(|e| anyhow!("could not create response: {}", e))?);
    }

    Ok(next.run(request).await)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Router, body::Body, http::Request, middleware::from_fn, routing::get};
    use tower::ServiceExt;

    #[tokio::test]
    async fn test_missing_content_type_header_returns_400() {
        let app = Router::new()
            .route("/", get("OK"))
            .layer(from_fn(validate_content_type));

        let request = Request::builder()
            .uri("/".to_string())
            .method("GET")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), 400);
    }

    #[tokio::test]
    async fn test_content_type_header_invalid_type_returns_400() {
        let app = Router::new()
            .route("/", get("OK"))
            .layer(from_fn(validate_content_type));

        let request = Request::builder()
            .uri("/")
            .method("GET")
            .header(CONTENT_TYPE, "invalid/type")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), 400);
    }

    #[tokio::test]
    async fn test_content_type_header_xml_returns_400() {
        let app = Router::new()
            .route("/", get("OK"))
            .layer(from_fn(validate_content_type));

        let request = Request::builder()
            .uri("/")
            .method("GET")
            .header(CONTENT_TYPE, "application/xml")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), 400);
    }

    #[tokio::test]
    async fn test_content_type_header_html_returns_400() {
        let app = Router::new()
            .route("/", get("OK"))
            .layer(from_fn(validate_content_type));

        let request = Request::builder()
            .uri("/")
            .method("GET")
            .header(CONTENT_TYPE, "text/html")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), 400);
    }

    #[tokio::test]
    async fn test_body_too_large_returns_413() {
        let app = Router::new()
            .route("/", get("OK"))
            .layer(from_fn(validate_body_length));

        // Create a body larger than 1MB
        let large_body = vec![b'a'; 1024 * 1024 + 1];
        let request = Request::builder()
            .uri("/")
            .method("GET")
            .body(Body::from(large_body))
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), 413);
    }
}
