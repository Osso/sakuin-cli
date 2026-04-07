use anyhow::{Context, Result};
use prost::Message;
use reqwest::Client;

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

    pub async fn call<Req: Message, Resp: Message + Default>(
        &self,
        service: &str,
        method: &str,
        request: Req,
    ) -> Result<Resp> {
        let url = format!("{}/{}/{}", self.base_url, service, method);

        // Encode request with grpc-web framing (1 byte flag + 4 bytes length + message)
        let msg_bytes = request.encode_to_vec();
        let mut body = Vec::with_capacity(5 + msg_bytes.len());
        body.push(0); // compression flag = 0 (not compressed)
        body.extend_from_slice(&(msg_bytes.len() as u32).to_be_bytes());
        body.extend_from_slice(&msg_bytes);

        let mut req = self
            .client
            .post(&url)
            .header("Content-Type", "application/grpc-web+proto")
            .header("Accept", "application/grpc-web+proto")
            .header("x-grpc-web", "1");

        if let Some(token) = &self.token {
            req = req.header("Authorization", format!("Bearer {}", token));
        }

        let resp = req.body(body).send().await.context("HTTP request failed")?;

        let status = resp.status();
        let resp_bytes = resp.bytes().await.context("Failed to read response")?;

        // Check for grpc-status in trailers (for grpc-web-text, trailers are in body)
        // For binary grpc-web, we need to parse the frames

        if resp_bytes.len() < 5 {
            anyhow::bail!(
                "Response too short: {} bytes, status: {}",
                resp_bytes.len(),
                status
            );
        }

        // Parse grpc-web response frames
        let mut offset = 0;
        let mut message_data = None;
        let mut grpc_status: Option<i32> = None;
        let mut grpc_message: Option<String> = None;

        while offset < resp_bytes.len() {
            if offset + 5 > resp_bytes.len() {
                break;
            }

            let flag = resp_bytes[offset];
            let len = u32::from_be_bytes([
                resp_bytes[offset + 1],
                resp_bytes[offset + 2],
                resp_bytes[offset + 3],
                resp_bytes[offset + 4],
            ]) as usize;

            offset += 5;

            if offset + len > resp_bytes.len() {
                anyhow::bail!("Invalid frame length");
            }

            let frame_data = &resp_bytes[offset..offset + len];
            offset += len;

            if flag & 0x80 != 0 {
                // Trailer frame
                let trailer_str = String::from_utf8_lossy(frame_data);
                for line in trailer_str.lines() {
                    if let Some((key, value)) = line.split_once(':') {
                        let key = key.trim().to_lowercase();
                        let value = value.trim();
                        if key == "grpc-status" {
                            grpc_status = value.parse().ok();
                        } else if key == "grpc-message" {
                            grpc_message = Some(value.to_string());
                        }
                    }
                }
            } else {
                // Data frame
                message_data = Some(frame_data.to_vec());
            }
        }

        // Check grpc status
        if let Some(status) = grpc_status {
            if status != 0 {
                let msg = grpc_message.unwrap_or_else(|| format!("gRPC error {}", status));
                anyhow::bail!("{}", msg);
            }
        }

        let data = message_data.context("No message data in response")?;
        let response = Resp::decode(data.as_slice()).context("Failed to decode response")?;

        Ok(response)
    }
}
