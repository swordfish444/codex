use base64::Engine as _;

pub(crate) fn parse_jwt_claims(token: &str) -> serde_json::Value {
    let mut parts = token.split('.');
    let _header = parts.next();
    let payload = parts.next();
    let _sig = parts.next();
    match payload {
        Some(p) if !p.is_empty() => decode_jwt_payload_segment(p),
        _ => serde_json::Value::Object(Default::default()),
    }
}

fn decode_jwt_payload_segment(segment_b64: &str) -> serde_json::Value {
    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(segment_b64)
        .ok();
    decoded
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        .unwrap_or(serde_json::Value::Object(Default::default()))
}
