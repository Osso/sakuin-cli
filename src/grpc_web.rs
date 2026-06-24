use anyhow::{Context, Result};
use prost::Message;
use reqwest::Client;

const FRAME_HEADER_LEN: usize = 5;
const TRAILER_FRAME_FLAG: u8 = 0x80;

pub struct GrpcWebClient {
    client: Client,
    base_url: String,
    token: Option<String>,
}

impl GrpcWebClient {
    pub fn new(base_url: String, token: Option<String>) -> Self {
        Self {
            client: Client::new(),
            base_url,
            token,
        }
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub async fn call<Req: Message, Resp: Message + Default>(
        &self,
        service: &str,
        method: &str,
        request: Req,
    ) -> Result<Resp> {
        let url = format!("{}/{}/{}", self.base_url, service, method);
        let request_body = encode_request_body(request);
        let response_bytes = self.send_request(&url, request_body).await?;
        ensure_frame_present(&response_bytes)?;
        let parsed_frames = parse_response_frames(&response_bytes)?;
        ensure_grpc_success(parsed_frames.grpc_status, parsed_frames.grpc_message)?;
        decode_response_message(parsed_frames.message_data)
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    async fn send_request(&self, url: &str, body: Vec<u8>) -> Result<Vec<u8>> {
        let request = self.build_request(url);
        let response = request
            .body(body)
            .send()
            .await
            .context("HTTP request failed")?;
        let response_bytes = response.bytes().await.context("Failed to read response")?;
        Ok(response_bytes.to_vec())
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    fn build_request(&self, url: &str) -> reqwest::RequestBuilder {
        let mut request = self
            .client
            .post(url)
            .header("Content-Type", "application/grpc-web+proto")
            .header("Accept", "application/grpc-web+proto")
            .header("x-grpc-web", "1");

        if let Some(token) = &self.token {
            request = request.header("Authorization", format!("Bearer {}", token));
        }

        request
    }
}

#[derive(Default)]
struct ParsedFrames {
    message_data: Option<Vec<u8>>,
    grpc_status: Option<i32>,
    grpc_message: Option<String>,
}

struct Frame<'a> {
    flag: u8,
    data: &'a [u8],
}

fn encode_request_body<Req: Message>(request: Req) -> Vec<u8> {
    let message_bytes = request.encode_to_vec();
    let mut body = Vec::with_capacity(FRAME_HEADER_LEN + message_bytes.len());
    body.push(0);
    body.extend_from_slice(&(message_bytes.len() as u32).to_be_bytes());
    body.extend_from_slice(&message_bytes);
    body
}

fn ensure_frame_present(response_bytes: &[u8]) -> Result<()> {
    if response_bytes.len() < FRAME_HEADER_LEN {
        anyhow::bail!("Response too short: {} bytes", response_bytes.len());
    }
    Ok(())
}

fn parse_response_frames(response_bytes: &[u8]) -> Result<ParsedFrames> {
    let mut parsed = ParsedFrames::default();
    let mut offset = 0;

    while let Some(frame) = next_frame(response_bytes, &mut offset)? {
        if is_trailer_frame(frame.flag) {
            parse_trailer_lines(frame.data, &mut parsed);
            continue;
        }
        parsed.message_data = Some(frame.data.to_vec());
    }

    Ok(parsed)
}

fn next_frame<'a>(response_bytes: &'a [u8], offset: &mut usize) -> Result<Option<Frame<'a>>> {
    if *offset >= response_bytes.len() {
        return Ok(None);
    }
    if *offset + FRAME_HEADER_LEN > response_bytes.len() {
        return Ok(None);
    }

    let flag = response_bytes[*offset];
    let frame_length = parse_frame_length(response_bytes, *offset + 1);
    let frame_start = *offset + FRAME_HEADER_LEN;
    let frame_end = frame_start + frame_length;

    if frame_end > response_bytes.len() {
        anyhow::bail!("Invalid frame length");
    }

    *offset = frame_end;
    Ok(Some(Frame {
        flag,
        data: &response_bytes[frame_start..frame_end],
    }))
}

fn parse_frame_length(response_bytes: &[u8], offset: usize) -> usize {
    u32::from_be_bytes([
        response_bytes[offset],
        response_bytes[offset + 1],
        response_bytes[offset + 2],
        response_bytes[offset + 3],
    ]) as usize
}

fn is_trailer_frame(flag: u8) -> bool {
    flag & TRAILER_FRAME_FLAG != 0
}

fn parse_trailer_lines(frame_data: &[u8], parsed: &mut ParsedFrames) {
    let trailer_text = String::from_utf8_lossy(frame_data);
    for line in trailer_text.lines() {
        update_status_from_trailer_line(line, parsed);
    }
}

fn update_status_from_trailer_line(line: &str, parsed: &mut ParsedFrames) {
    let Some((key, value)) = line.split_once(':') else {
        return;
    };

    match key.trim().to_lowercase().as_str() {
        "grpc-status" => parsed.grpc_status = value.trim().parse().ok(),
        "grpc-message" => parsed.grpc_message = Some(value.trim().to_string()),
        _ => {}
    }
}

fn ensure_grpc_success(status: Option<i32>, message: Option<String>) -> Result<()> {
    if let Some(code) = status {
        if code != 0 {
            let error_message = message.unwrap_or_else(|| format!("gRPC error {}", code));
            anyhow::bail!("{}", error_message);
        }
    }
    Ok(())
}

fn decode_response_message<Resp: Message + Default>(message_data: Option<Vec<u8>>) -> Result<Resp> {
    let payload = message_data.context("No message data in response")?;
    let response = Resp::decode(payload.as_slice()).context("Failed to decode response")?;
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::{GetStatsRequest, Stats};

    fn frame(flag: u8, payload: &[u8]) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(FRAME_HEADER_LEN + payload.len());
        bytes.push(flag);
        bytes.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        bytes.extend_from_slice(payload);
        bytes
    }

    #[test]
    fn encode_request_body_wraps_protobuf_message_in_grpc_web_frame() {
        let body = encode_request_body(GetStatsRequest {});

        assert_eq!(body[0], 0);
        assert_eq!(parse_frame_length(&body, 1), 0);
        assert_eq!(body.len(), FRAME_HEADER_LEN);
    }

    #[test]
    fn parse_response_frames_extracts_message_and_trailers() {
        let message = Stats {
            cleaned_manga: 10,
            raw_manga: 20,
        }
        .encode_to_vec();
        let trailers = b"grpc-status: 0\r\ngrpc-message: ok\r\n";
        let mut response = frame(0, &message);
        response.extend_from_slice(&frame(TRAILER_FRAME_FLAG, trailers));

        let parsed = parse_response_frames(&response).unwrap();
        let decoded: Stats = decode_response_message(parsed.message_data).unwrap();

        assert_eq!(decoded.cleaned_manga, 10);
        assert_eq!(decoded.raw_manga, 20);
        assert_eq!(parsed.grpc_status, Some(0));
        assert_eq!(parsed.grpc_message.as_deref(), Some("ok"));
    }

    #[test]
    fn next_frame_rejects_invalid_lengths_and_ignores_short_headers() {
        let mut offset = 0;
        assert!(next_frame(&[0, 0], &mut offset).unwrap().is_none());

        let invalid = [0, 0, 0, 0, 10, 1, 2];
        let mut offset = 0;
        assert!(next_frame(&invalid, &mut offset).is_err());
    }

    #[test]
    fn ensure_grpc_success_reports_nonzero_status() {
        assert!(ensure_grpc_success(Some(0), None).is_ok());
        assert_eq!(
            ensure_grpc_success(Some(7), Some("permission denied".to_string()))
                .unwrap_err()
                .to_string(),
            "permission denied"
        );
        assert!(
            ensure_grpc_success(Some(13), None)
                .unwrap_err()
                .to_string()
                .contains("gRPC error 13")
        );
    }

    #[test]
    fn decode_response_message_requires_message_data() {
        let err = decode_response_message::<Stats>(None).unwrap_err();

        assert!(err.to_string().contains("No message data"));
    }
}
