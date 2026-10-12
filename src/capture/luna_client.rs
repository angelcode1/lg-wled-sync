//! Read-only Luna request helper for webOS versions where luna-send -n 1
//! silently exits without a response. On LG C5 / SDK 10, -i with *open*
//! stdin returns JSON; the process must be killed and reaped after receipt.
//! Only requests that expect one JSON reply are supported here.
use serde_json::Value;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio::process::Command;

/// Make one private-bus request without blocking the calling capture loop.
/// stdout is capped; no shell interpretation, TV writes, or service restarts.
pub async fn request(uri: &str, payload: &str, timeout: Duration, max_bytes: usize) -> Option<Value> {
    let mut command = Command::new("luna-send");
    command.args(["-i", "-f", uri, payload]);
    read_interactive_reply(&mut command, timeout, max_bytes).await
}

/// Exposed to internal tests for mimicking an interactive Luna child process.
async fn read_interactive_reply(
    command: &mut Command,
    timeout: Duration,
    max_bytes: usize,
) -> Option<Value> {
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);

    let mut child = command.spawn().ok()?;
    // IMPORTANT: do NOT use child.wait_with_output() here: it closes stdin,
    // which reproduces the no-response behaviour of luna-send -n 1 on C5.
    // Keeping this handle open is what allows the interactive Luna reply.
    let stdin = child.stdin.take();
    let mut stdout = child.stdout.take()?;

    let response = tokio::time::timeout(timeout, async {
        let mut buffer = Vec::with_capacity(4096);
        let mut chunk = [0u8; 4096];
        loop {
            let received = stdout.read(&mut chunk).await.ok()?;
            if received == 0 {
                return None;
            }
            if buffer.len().saturating_add(received) > max_bytes {
                return None;
            }
            buffer.extend_from_slice(&chunk[..received]);
            if let Some(start) = buffer.iter().position(|&c| c == b'{' || c == b'[') {
                if let Ok(reply) = serde_json::from_slice::<Value>(&buffer[start..]) {
                    return Some(reply);
                }
                // An incomplete JSON document is normal between reads.
            }
        }
    })
    .await
    .ok()
    .flatten();

    // Stop only our Luna client; never signal the bus or service. Reap it
    // after closing stdin, even after timeout/oversized/malformed replies.
    drop(stdin);
    let _ = child.start_kill();
    let _ = tokio::time::timeout(Duration::from_millis(400), child.wait()).await;
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn parses_json_before_interactive_client_exits() {
        let mut command = Command::new("sh");
        command.args([
            "-c",
            r#"printf '%s\n' '{"returnValue":true,"modelName":"OLED42C5PSA.AAUQLJD"}'; read unused"#,
        ]);
        let parsed = read_interactive_reply(&mut command, Duration::from_secs(2), 4096)
            .await
            .expect("interactive child should deliver JSON while stdin is open");
        assert_eq!(parsed["modelName"], "OLED42C5PSA.AAUQLJD");
    }

    #[tokio::test]
    async fn parses_array_reply_in_chunks() {
        let mut command = Command::new("sh");
        command.args([
            "-c",
            r#"printf '['; printf '%s' '{"resource":[{"resource":"HDMI_INPUT"}]}'; printf ']'; read unused"#,
        ]);
        let parsed = read_interactive_reply(&mut command, Duration::from_secs(2), 4096)
            .await
            .expect("array reply");
        assert!(parsed.is_array());
    }

    #[tokio::test]
    async fn bounded_timeout_reaps_silent_child() {
        let mut command = Command::new("sh");
        command.args(["-c", "read unused"]);
        assert!(read_interactive_reply(&mut command, Duration::from_millis(100), 1024)
            .await
            .is_none());
    }

    #[tokio::test]
    async fn oversized_reply_is_rejected() {
        let mut command = Command::new("sh");
        command.args(["-c", r#"printf '%s' '{"large":"01234567890123456789"}'; read unused"#]);
        assert!(read_interactive_reply(&mut command, Duration::from_secs(2), 12)
            .await
            .is_none());
    }
}
