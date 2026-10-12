//! LG media-resource classifier. Based on the resource envelope documented
//! in HyperDRM 0.13.0 (native/playback.h), not app-name or black-frame guessing.
//! Only a foreground media/VDEC pipeline without secure resources is clear.
//! HDMI, idle, unrecognised envelopes and failed Luna calls remain unknown.
use super::drm_routing::PlaybackClass;
use crate::capture::luna_client;
use serde_json::Value;
use std::time::Duration;

pub fn classify(value: &Value) -> PlaybackClass {
    let Some(pipelines) = value.as_array() else {
        return PlaybackClass::Unknown;
    };
    let (mut secure, mut clear, mut malformed) = (false, false, false);
    for pipeline in pipelines {
        let Some(fields) = pipeline.as_object() else {
            malformed = true;
            continue;
        };
        let Some(resources) = fields
            .get("resource")
            .or_else(|| fields.get("resources"))
            .and_then(Value::as_array)
        else {
            malformed = true;
            continue;
        };
        let mut video = false;
        for resource in resources {
            let Some(name) = resource.get("resource").and_then(Value::as_str) else {
                malformed = true;
                continue;
            };
            if name == "SVP_CPB" {
                secure = true;
            }
            if name == "VDEC" {
                video = true;
            }
        }
        if video
            && fields.get("is_foreground").and_then(Value::as_bool) == Some(true)
            && fields.get("type").and_then(Value::as_str) == Some("media")
        {
            clear = true;
        }
    }
    if secure {
        PlaybackClass::Protected
    } else if clear && !malformed {
        PlaybackClass::Clear
    } else {
        PlaybackClass::Unknown
    }
}

pub async fn query() -> PlaybackClass {
    // LG C5 returns empty stdout from luna-send -n 1 even when exit=0;
    // interactive -i with stdin kept open returns the actual JSON array.
    let Some(reply) = luna_client::request(
        "luna://com.webos.media/getActivePipelines",
        "{}",
        Duration::from_millis(2300),
        128 * 1024,
    )
    .await
    else {
        return PlaybackClass::Unknown;
    };
    classify(&reply)
}

const MODEL_URI: &str = "luna://com.webos.service.tv.systemproperty/getSystemInfo";

pub fn parse_model_reply(raw: &str) -> Option<String> {
    let start = raw.find('{')?;
    let object: Value = serde_json::from_str(raw[start..].trim()).ok()?;
    if object.get("returnValue").and_then(Value::as_bool) != Some(true) {
        return None;
    }
    let model = object.get("modelName")?.as_str()?;
    if model.is_empty()
        || model.len() > 63
        || !model
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | ' '))
    {
        return None;
    }
    Some(model.to_string())
}

/// Read-only system-property query, guarded against hung Luna services.
pub async fn query_tv_model() -> Option<String> {
    let reply = luna_client::request(
        MODEL_URI,
        r#"{"keys":["modelName"]}"#,
        Duration::from_millis(2300),
        8192,
    )
    .await?;
    parse_model_reply(&reply.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn model_reply_must_be_exact_and_successful() {
        assert_eq!(
            parse_model_reply(r#"{"returnValue":true,"modelName":"OLED65C5PSA"}"#),
            Some("OLED65C5PSA".to_string())
        );
        assert_eq!(
            parse_model_reply("info\n{\"returnValue\":false,\"modelName\":\"OLED65C5PSA\"}"),
            None
        );
        assert_eq!(
            parse_model_reply(r#"{"returnValue":true,"modelName":"OLED65C5PSA"} trailing"#),
            None
        );
        assert_eq!(
            parse_model_reply(r#"{"returnValue":true,"modelName":"INVALID/TV"}"#),
            None
        );
    }
    #[test]
    fn exact_c5_model_and_hdmi_pipeline_fixture() {
        assert_eq!(
            parse_model_reply(
                r#"{"returnValue":true,"modelName":"OLED42C5PSA.AAUQLJD","sdkVersion":"10.0.0"}"#
            ),
            Some("OLED42C5PSA.AAUQLJD".to_string())
        );
        let hdmi = json!([
            {"is_foreground":false, "type":"vtclient", "resource":[]},
            {"is_foreground":true, "type":"avconnector", "resource":[{"resource":"MAIN_SCALER"}]},
            {"is_foreground":true, "type":"tv", "currentAppId":"com.webos.app.hdmi1",
             "resource":[{"resource":"HDMI_INPUT"},{"resource":"VHDMIRX"},{"resource":"AHDMIRX"},
                         {"resource":"ADEC"},{"resource":"ADEC_BANDWIDTH"}]}
        ]);
        // HDMI routing is not evidence that the HDMI source lacks HDCP.
        assert_eq!(classify(&hdmi), PlaybackClass::Unknown);
    }
    #[test]
    fn valid_clear_requires_foreground_media_decoder() {
        assert_eq!(
            classify(&json!([{"is_foreground":true,"type":"media",
            "resource":[{"resource":"VDEC"}]}])),
            PlaybackClass::Clear
        );
        assert_eq!(
            classify(&json!([{"is_foreground":true,"type":"hdmi",
            "resource":[{"resource":"VDEC"}]}])),
            PlaybackClass::Unknown
        );
        assert_eq!(
            classify(&json!([{"is_foreground":false,"type":"media",
            "resources":[{"resource":"VDEC"}]}])),
            PlaybackClass::Unknown
        );
    }
    #[test]
    fn secure_resource_always_wins_even_with_malformed_other_pipeline() {
        assert_eq!(
            classify(&json!([{},{"resource":[{"resource":"SVP_CPB"}]}])),
            PlaybackClass::Protected
        );
        assert_eq!(
            classify(&json!([{"is_foreground":true,"type":"media",
            "resource":[{"resource":"VDEC"},{"resource":"SVP_CPB"}]}])),
            PlaybackClass::Protected
        );
        assert_eq!(
            classify(&json!([{"is_foreground":true,"type":"media",
            "resource":[{"resource":"VDEC"}]},{}])),
            PlaybackClass::Unknown
        );
    }
    #[test]
    fn unknown_when_data_is_invalid_or_idle() {
        assert_eq!(classify(&json!([])), PlaybackClass::Unknown);
        assert_eq!(
            classify(&json!({"returnValue":false})),
            PlaybackClass::Unknown
        );
    }
}
