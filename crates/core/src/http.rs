use crate::{
    CancellationToken,
    domain::{DownloadOptions, Remote, Validator},
};
use anyhow::{Context, Result, bail, ensure};
use reqwest::{Client, Response, StatusCode, header::*};
use std::time::Duration;

/// Owns HTTP configuration and representation inspection.
#[derive(Clone)]
pub(crate) struct HttpSource {
    client: Client,
}
impl HttpSource {
    pub(crate) fn new(options: &DownloadOptions) -> Result<Self> {
        let mut headers = HeaderMap::new();
        for (name, value) in &options.headers {
            let name: HeaderName = name.parse().context("Invalid request header name")?;
            ensure!(
                name != RANGE
                    && name != IF_RANGE
                    && name != ACCEPT_ENCODING
                    && name != CONTENT_LENGTH,
                "Range, If-Range, Accept-Encoding and Content-Length are managed by the engine"
            );
            let mut value: HeaderValue = value.parse().context("Invalid request header value")?;
            value.set_sensitive(true);
            headers.append(name, value);
        }
        headers.insert(ACCEPT_ENCODING, HeaderValue::from_static("identity"));
        if !headers.contains_key(USER_AGENT) {
            headers.insert(
                USER_AGENT,
                HeaderValue::from_static(concat!("rustdm/", env!("CARGO_PKG_VERSION"))),
            );
        }
        let client = Client::builder()
            .default_headers(headers)
            .connect_timeout(Duration::from_secs(15))
            .read_timeout(Duration::from_secs(30))
            .build()?;
        Ok(Self { client })
    }
    pub(crate) fn client(&self) -> &Client {
        &self.client
    }
    pub(crate) async fn inspect(
        &self,
        url: &str,
        cancel: &CancellationToken,
        retries: u32,
    ) -> Result<Remote> {
        let mut inspection = self.probe(url, cancel).await;
        for attempt in 0..retries {
            if inspection.is_ok() || cancel.is_cancelled() {
                break;
            }
            tokio::select! {
                _ = cancel.cancelled() => bail!("Download paused"),
                _ = tokio::time::sleep(Duration::from_millis(250 * (1 << attempt.min(4)))) => {},
            }
            inspection = self.probe(url, cancel).await;
        }
        inspection.context("Cannot inspect download")
    }
    async fn probe(&self, url: &str, cancel: &CancellationToken) -> Result<Remote> {
        let response = tokio::select! {
            _ = cancel.cancelled() => bail!("Download paused"),
            response = self.client.get(url).header(RANGE, "bytes=0-0").send() => response?,
        };
        identity(&response)?;
        let final_url = response.url().to_string();
        let validator = validator(response.headers());
        if response.status() == StatusCode::RANGE_NOT_SATISFIABLE
            && response
                .headers()
                .get(CONTENT_RANGE)
                .is_some_and(|v| v == "bytes */0")
        {
            return Ok(Remote {
                final_url,
                size: Some(0),
                ranges: false,
                validator,
            });
        }
        if response.status() == StatusCode::PARTIAL_CONTENT {
            let (start, end, total) = range_header(&response)?;
            ensure!(
                start == 0 && end == 0,
                "Server ignored the probe's requested range"
            );
            return Ok(Remote {
                final_url,
                size: Some(total),
                ranges: true,
                validator,
            });
        }
        ensure!(
            response.status() == StatusCode::OK,
            "Probe returned HTTP {}",
            response.status()
        );
        Ok(Remote {
            final_url,
            size: response.content_length(),
            ranges: false,
            validator,
        })
    }
}
pub(crate) fn text_header(headers: &HeaderMap, name: HeaderName) -> Option<String> {
    headers.get(name)?.to_str().ok().map(str::to_owned)
}

pub(crate) fn validator(headers: &HeaderMap) -> Option<Validator> {
    if let Some(value) = text_header(headers, ETAG)
        .filter(|v| v.starts_with('"') && v.ends_with('"') && v.len() >= 2)
    {
        return Some(Validator {
            header: "etag".into(),
            value,
        });
    }
    text_header(headers, LAST_MODIFIED).map(|value| Validator {
        header: "last-modified".into(),
        value,
    })
}

pub(crate) fn identity(response: &Response) -> Result<()> {
    if let Some(value) = response.headers().get(CONTENT_ENCODING) {
        ensure!(
            value == "identity",
            "Server returned an encoded representation despite Accept-Encoding: identity"
        );
    }
    Ok(())
}

fn parse_content_range(value: &str) -> Result<(u64, u64, u64)> {
    let rest = value
        .strip_prefix("bytes ")
        .context("Invalid Content-Range unit")?;
    let (bounds, total) = rest
        .split_once('/')
        .context("Missing Content-Range total")?;
    let (start, end) = bounds
        .split_once('-')
        .context("Missing Content-Range endpoints")?;
    let (start, end, total): (u64, u64, u64) = (start.parse()?, end.parse()?, total.parse()?);
    ensure!(start <= end && end < total, "Invalid Content-Range bounds");
    Ok((start, end, total))
}

pub(crate) fn range_header(response: &Response) -> Result<(u64, u64, u64)> {
    parse_content_range(
        response
            .headers()
            .get(CONTENT_RANGE)
            .context("Partial response omitted Content-Range")?
            .to_str()?,
    )
}
