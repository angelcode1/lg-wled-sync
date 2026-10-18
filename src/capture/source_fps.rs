pub fn detect_source_fps() -> Option<f64> {
    if std::path::Path::new("/usr/bin/luna-send").exists() {
        if let Ok(output) = std::process::Command::new("/usr/bin/luna-send")
            .args([
                "-n",
                "1",
                "luna://com.webos.service.tv.display/getVideoInfo",
                "{}",
            ])
            .output()
        {
            if output.status.success() {
                if let Ok(val) = serde_json::from_slice::<serde_json::Value>(&output.stdout) {
                    return source_fps_from_video_info(&val);
                }
            }
        }
    }
    None
}

fn source_fps_from_video_info(value: &serde_json::Value) -> Option<f64> {
    match value {
        serde_json::Value::Object(fields) => fields
            .get("frameRate")
            .and_then(parse_frame_rate)
            .or_else(|| fields.values().find_map(source_fps_from_video_info)),
        serde_json::Value::Array(values) => values.iter().find_map(source_fps_from_video_info),
        _ => None,
    }
}

fn parse_frame_rate(value: &serde_json::Value) -> Option<f64> {
    if let Some(value) = value.as_f64() {
        return value.is_finite().then_some(value);
    }
    let value = value.as_str()?.trim();
    if let Some((numerator, denominator)) = value.split_once('/') {
        let numerator = numerator.trim().parse::<f64>().ok()?;
        let denominator = denominator.trim().parse::<f64>().ok()?;
        return (denominator != 0.0).then_some(numerator / denominator);
    }
    value.parse::<f64>().ok().filter(|value| value.is_finite())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_nested_fractional_video_frame_rate() {
        let video_info = serde_json::json!({"payload": {"frameRate": "24000/1001"}});
        assert!((source_fps_from_video_info(&video_info).unwrap() - 23.976).abs() < 0.001);
    }
}
