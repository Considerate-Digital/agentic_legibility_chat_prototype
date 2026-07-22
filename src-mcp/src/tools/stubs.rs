pub fn rand_u32() -> u32 {
    // Simple deterministic stub — not cryptographically random
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0)
}

pub fn fetch(args: &serde_json::Value) -> String {
    let url = match args["url"].as_str() {
        Some(u) => u,
        None => return "[fetch error] missing required 'url' argument".to_string(),
    };
    let method = args["method"].as_str().unwrap_or("GET").to_uppercase();

    let mut request = ureq::request(&method, url);

    if let Some(headers_str) = args["headers"].as_str() {
        if let Ok(headers_obj) = serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(headers_str) {
            for (key, val) in &headers_obj {
                if let Some(v) = val.as_str() {
                    request = request.set(key, v);
                }
            }
        }
    }

    let body_str = args["body"].as_str().unwrap_or("");

    let response = if body_str.is_empty() {
        request.call()
    } else {
        request.send_string(body_str)
    };

    match response {
        Ok(resp) => {
            let status = resp.status();
            let body = resp.into_string().unwrap_or_else(|e| format!("[body read error: {}]", e));
            let truncated = if body.len() > 4000 {
                format!("{}\n... [truncated {} bytes]", &body[..4000], body.len() - 4000)
            } else {
                body
            };
            format!("HTTP {}\n{}", status, truncated)
        }
        Err(e) => format!("[fetch error] {}", e),
    }
}

