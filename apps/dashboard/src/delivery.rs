//! Compress public responses before accounting for outbound bytes. Limits stay unchanged.
use axum::{
    body::Bytes,
    http::{HeaderMap, HeaderValue, header},
};
use flate2::{Compression, write::GzEncoder};
use std::io::Write;

pub fn accepts_gzip(value: &str) -> bool {
    let (mut gzip, mut wildcard) = (None, None);
    for entry in value.split(',') {
        let mut parts = entry.split(';');
        let encoding = parts.next().unwrap_or_default().trim();
        let mut quality = 1.0_f32;
        for parameter in parts {
            if let Some((name, value)) = parameter.trim().split_once('=')
                && name.trim().eq_ignore_ascii_case("q")
            {
                quality = value
                    .trim()
                    .parse::<f32>()
                    .ok()
                    .filter(|q| q.is_finite() && (0.0..=1.0).contains(q))
                    .unwrap_or(0.0);
            }
        }
        if encoding.eq_ignore_ascii_case("gzip") {
            gzip = Some(gzip.unwrap_or(true) && quality > 0.0);
        } else if encoding == "*" {
            wildcard = Some(wildcard.unwrap_or(true) && quality > 0.0);
        }
    }
    gzip.or(wildcard).unwrap_or(false)
}

pub fn encode(headers: &mut HeaderMap, body: Bytes, gzip: bool) -> Bytes {
    let kind = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    let eligible = kind.starts_with("text/")
        || kind.starts_with("application/json")
        || kind.starts_with("application/javascript")
        || kind.starts_with("image/svg+xml");
    if !eligible || headers.contains_key(header::CONTENT_ENCODING) || body.len() < 512 {
        return body;
    }
    headers.append(header::VARY, HeaderValue::from_static("Accept-Encoding"));
    if !gzip {
        return body;
    }
    let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
    if encoder.write_all(&body).is_err() {
        return body;
    }
    match encoder.finish() {
        Ok(encoded) if encoded.len() < body.len() => {
            headers.insert(header::CONTENT_ENCODING, HeaderValue::from_static("gzip"));
            headers.remove(header::CONTENT_LENGTH);
            headers.remove(header::ETAG);
            Bytes::from(encoded)
        }
        _ => body,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    #[test]
    fn negotiation_respects_explicit_refusal_and_quality() {
        for value in ["gzip", "br, gzip;q=0.5", "GZIP", "*;q=1"] {
            assert!(accepts_gzip(value));
        }
        for value in [
            "",
            "br",
            "gzip;q=0",
            "gzip;q=0, *;q=1",
            "gzip;q=oops",
            "gzip;q=2",
            "gzip;q=NaN",
        ] {
            assert!(!accepts_gzip(value));
        }
    }
    #[test]
    fn html_round_trip_preserves_bytes_and_varies_the_cache() {
        let original = Bytes::from_static(include_bytes!("../public/index.html"));
        let mut headers = HeaderMap::new();
        headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("text/html; charset=utf-8"),
        );
        headers.insert(header::CONTENT_LENGTH, HeaderValue::from_static("107948"));
        let compressed = encode(&mut headers, original.clone(), true);
        assert!(compressed.len() < original.len() / 2);
        assert_eq!(headers[header::CONTENT_ENCODING], "gzip");
        assert!(!headers.contains_key(header::CONTENT_LENGTH));
        assert_eq!(headers[header::VARY], "Accept-Encoding");
        let mut decoded = Vec::new();
        flate2::read::GzDecoder::new(&compressed[..])
            .read_to_end(&mut decoded)
            .unwrap();
        assert_eq!(decoded, original);
    }
    #[test]
    fn identity_and_binary_responses_remain_usable_without_double_encoding() {
        let original = Bytes::from(vec![b'a'; 2048]);
        let mut headers = HeaderMap::new();
        headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        assert_eq!(encode(&mut headers, original.clone(), false), original);
        assert!(!headers.contains_key(header::CONTENT_ENCODING));
        headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("image/png"));
        assert_eq!(encode(&mut headers, original.clone(), true), original);
        headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        headers.insert(header::CONTENT_ENCODING, HeaderValue::from_static("br"));
        assert_eq!(encode(&mut headers, original.clone(), true), original);
        assert_eq!(headers[header::CONTENT_ENCODING], "br");
    }
}
