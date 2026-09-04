//! Static Instagram device/app profile constants.
//!
//! Ported from instagrapi `config.py` (Instagram 428.0.0.47.67 / Pixel 8 Pro).

pub const API_DOMAIN: &str = "i.instagram.com";
pub const APP_ID: &str = "567067343352427";
pub const DEFAULT_APP_VERSION: &str = "428.0.0.47.67";

pub const USER_AGENT_BASE: &str = "Instagram {app_version} Android ({android_version}/{android_release}; {dpi}; {resolution}; {manufacturer}; {model}; {device}; {cpu}; {locale}; {version_code})";

/// Default device profile (hardware/system) settings.
pub const DEVICE_SETTINGS: &[(&str, &str)] = &[
    ("android_version", "34"),
    ("android_release", "14"),
    ("dpi", "480dpi"),
    ("resolution", "1344x2992"),
    ("manufacturer", "Google/google"),
    ("device", "husky"),
    ("model", "Pixel 8 Pro"),
    ("cpu", "husky"),
];

pub const APP_SETTINGS: &[(&str, &str, &str)] = &[
    (
        "428.0.0.47.67",
        "961145276",
        "7189b949425f9bf80ea8bd880cf5a3080b292d9b1c4b38a18d112f7c4b71e7a8",
    ),
    (
        "364.0.0.35.86",
        "374010953",
        "8ccf54aad76788a6ca03ddfc33afcdcf692f2f5a3ba814ea73d5facba7fa2c2d",
    ),
    (
        "385.0.0.47.74",
        "378906843",
        "a8973d49a9cc6a6f65a4997c10216ce2a06f65a517010e64885e92029bb19221",
    ),
];

pub const LOGIN_EXPERIMENTS: &str = "ig_android_reg_nux_headers_cleanup_universe,\
ig_android_device_detection_info_upload,\
ig_android_nux_add_email_device,\
ig_android_gmail_oauth_in_reg,\
ig_android_device_info_foreground_reporting,\
ig_android_device_verification_fb_signup,\
ig_android_direct_main_tab_universe_v2,\
ig_android_passwordless_account_password_creation_universe,\
ig_android_direct_add_direct_to_android_native_photo_share_sheet,\
ig_growth_android_profile_pic_prefill_with_fb_pic_2,\
ig_account_identity_logged_out_signals_global_holdout_universe,\
ig_android_quickcapture_keep_screen_on,\
ig_android_device_based_country_verification,\
ig_android_login_identifier_fuzzy_match,\
ig_android_reg_modularization_universe,\
ig_android_security_intent_switchoff,\
ig_android_device_verification_separate_endpoint,\
ig_android_suma_landing_page,\
ig_android_sim_info_upload,\
ig_android_smartlock_hints_universe,\
ig_android_fb_account_linking_sampling_freq_universe,\
ig_android_retry_create_account_universe,\
ig_android_caption_typeahead_fix_on_o_universe";

pub const SUPPORTED_CAPABILITIES: &str = r#"[{"value":"119.0,120.0,121.0,122.0,123.0,124.0,125.0,126.0,127.0,128.0,129.0,130.0,131.0,132.0,133.0,134.0,135.0,136.0,137.0,138.0,139.0,140.0,141.0,142.0","name":"SUPPORTED_SDK_VERSIONS"},{"value":"14","name":"FACE_TRACKER_VERSION"},{"value":"ETC2_COMPRESSION","name":"COMPRESSION"},{"value":"gyroscope_enabled","name":"gyroscope"}]"#;
