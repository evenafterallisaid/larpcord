use base64::Engine as _;

pub const BADGE_SCHEMA_BUILD: u32 = 676376;

pub fn build_from_app_shell(bytes: &[u8]) -> Option<u32> {
    let page = std::str::from_utf8(bytes).ok()?;
    let (_, script) = page.split_once("GLOBAL_ENV")?;
    let (_, value) = script.split_once("\"BUILD_NUMBER\":\"")?;
    let (number, _) = value.split_once('"')?;
    (number.len() >= 5 && number.len() <= 9 && number.bytes().all(|b| b.is_ascii_digit()))
        .then(|| number.parse().ok())
        .flatten()
}

pub fn compatibility_header(build: u32) -> String {
    let os = match std::env::consts::OS {
        "macos" => "Mac OS X",
        "windows" => "Windows",
        _ => "Linux",
    };
    base64::engine::general_purpose::STANDARD.encode(
        serde_json::json!({
            "os": os,
            "browser": "Larpcord",
            "device": "",
            "release_channel": "stable",
            "client_build_number": build,
            "client_version": env!("CARGO_PKG_VERSION"),
            "system_locale": "en-US"
        })
        .to_string(),
    )
}

pub fn catalog_number(id: &str) -> Option<u32> {
    if let Some(id) = id.strip_prefix("catalog-") {
        return id.parse().ok();
    }
    if id == "premium" || id.starts_with("premium_tenure_") {
        return Some(1);
    }
    if id.starts_with("guild_booster_lvl") {
        return Some(12);
    }
    [
        "premium_tenure",
        "partner",
        "certified_moderator",
        "hypesquad",
        "hypesquad_house_1",
        "hypesquad_house_2",
        "hypesquad_house_3",
        "bug_hunter_level_1",
        "bug_hunter_level_2",
        "verified_developer",
        "early_supporter",
        "guild_booster",
        "legacy_username",
        "quest_completed",
        "april_fools_2026",
        "orb_profile",
        "gifting",
        "account_age",
        "streaming",
        "game_time",
        "game_variety",
        "staff",
    ]
    .iter()
    .position(|name| *name == id)
    .map(|position| position as u32 + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_only_a_bounded_numeric_public_build() {
        assert_eq!(
            build_from_app_shell(br#"window.GLOBAL_ENV={"BUILD_NUMBER":"676376"}"#),
            Some(676376)
        );
        assert_eq!(build_from_app_shell(br#"{"BUILD_NUMBER":"676376"}"#), None);
        assert_eq!(
            build_from_app_shell(br#"window.GLOBAL_ENV={"BUILD_NUMBER":"676376unexpected"}"#),
            None
        );
        assert_eq!(
            catalog_number("premium_tenure_12_month_v2"),
            catalog_number("catalog-1")
        );
        assert_eq!(catalog_number("game_time"), catalog_number("catalog-20"));
    }
}
