//! Bound requests and compressed responses. Opening the dashboard has a dedicated allowance.
use axum::{
    body::Bytes,
    http::{HeaderMap, HeaderValue, header},
};
use flate2::{Compression, write::GzEncoder};
use std::{io::Write, sync::Mutex, time::Instant};

pub struct Bucket {
    capacity: f64,
    refill_per_second: f64,
    tokens: f64,
    at: Instant,
}

impl Bucket {
    fn new(capacity: usize, refill_per_second: usize, now: Instant) -> Self {
        Self {
            capacity: capacity as f64,
            refill_per_second: refill_per_second as f64,
            tokens: capacity as f64,
            at: now,
        }
    }

    pub fn allow(&mut self, now: Instant, amount: usize) -> bool {
        let refill = now.saturating_duration_since(self.at).as_secs_f64() * self.refill_per_second;
        self.tokens = (self.tokens + refill).min(self.capacity);
        self.at = now;
        if self.tokens < amount as f64 {
            return false;
        }
        self.tokens -= amount as f64;
        true
    }
}

pub struct Allowance {
    pub requests: Bucket,
    pub bytes: Bucket,
}

pub struct Budgets {
    page: Mutex<Allowance>,
    data: Mutex<Allowance>,
}

impl Budgets {
    pub fn new() -> Self {
        let now = Instant::now();
        Self {
            page: Mutex::new(Allowance {
                requests: Bucket::new(240, 8, now),
                bytes: Bucket::new(2 * 1024 * 1024, 8 * 1024, now),
            }),
            data: Mutex::new(Allowance {
                requests: Bucket::new(120, 8, now),
                bytes: Bucket::new(1024 * 1024, 16 * 1024, now),
            }),
        }
    }

    /// Only registered read routes may use capacity reserved for starting the page.
    pub fn for_request(
        &self,
        method: &axum::http::Method,
        route: Option<&str>,
    ) -> &Mutex<Allowance> {
        let page_read = matches!(*method, axum::http::Method::GET | axum::http::Method::HEAD)
            && route.is_some_and(|path| {
                matches!(
                    path,
                    "/" | "/api/assets" | "/api/research/quote-study" | "/api/health"
                ) || path.starts_with("/assets/")
            });
        if page_read { &self.page } else { &self.data }
    }
}

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
    fn exhausted_polling_cannot_starve_the_page_or_its_scripts() {
        use axum::http::Method;
        let budgets = Budgets::new();
        let now = Instant::now();
        let mut data = budgets
            .for_request(&Method::GET, Some("/api/feeds"))
            .lock()
            .unwrap();
        assert!(data.requests.allow(now, 120));
        assert!(!data.requests.allow(now, 1));
        assert!(data.bytes.allow(now, 1024 * 1024));
        assert!(!data.bytes.allow(now, 1));
        drop(data);
        for route in [
            "/",
            "/assets/homepage.js",
            "/api/assets",
            "/api/research/quote-study",
        ] {
            let mut page = budgets
                .for_request(&Method::GET, Some(route))
                .lock()
                .unwrap();
            assert!(page.requests.allow(now, 1));
            assert!(page.bytes.allow(now, 256 * 1024));
        }
        for (method, route) in [
            (Method::POST, Some("/")),
            (Method::GET, None),
            (Method::GET, Some("/api/outcomes")),
        ] {
            assert!(
                !budgets
                    .for_request(&method, route)
                    .lock()
                    .unwrap()
                    .requests
                    .allow(now, 1)
            );
        }
    }

    #[test]
    fn compressed_homepage_fits_with_64_kib_of_page_allowance_remaining() {
        let raw = Bytes::from_static(include_bytes!("../public/index.html"));
        let mut headers = HeaderMap::new();
        headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("text/html"));
        let compressed = encode(&mut headers, raw.clone(), true);
        let now = Instant::now();
        let mut budget = Bucket::new(64 * 1024, 8192, now);
        assert!(!budget.allow(now, raw.len()));
        assert!(budget.allow(now, compressed.len()));
    }

    #[test]
    fn buckets_refill_at_their_rate_and_do_not_accumulate_an_unbounded_burst() {
        let now = Instant::now();
        let mut b = Bucket::new(120, 8, now);
        assert!(b.allow(now, 120));
        let next = now + std::time::Duration::from_secs(1);
        assert!(b.allow(next, 8));
        assert!(!b.allow(next, 1));
        let later = next + std::time::Duration::from_secs(3600);
        assert!(!b.allow(later, 121));
        assert!(b.allow(later, 120));
    }

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
