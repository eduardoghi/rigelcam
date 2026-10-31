//! The languages of the interface. Texts are written in English, in QML and in the messages from
//! Rust, and the files in `i18n` hold their translations.

/// Separates a message from its arguments, which QML puts into its `%1` and `%2` once translated.
const ARGUMENT_SEPARATOR: char = '\u{1f}';

static PT_BR: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/rigelcam_pt_BR.qm"));
static ES: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/rigelcam_es.qm"));

extern "C" {
    fn rigel_system_language() -> i32;
    fn rigel_install_translation(data: *const u8, size: i32);
}

/// The language to use for a saved choice, where an empty one follows the system.
pub fn resolve(saved: &str) -> &'static str {
    match saved {
        "pt_BR" => "pt_BR",
        "es" => "es",
        "en" => "en",
        _ => match unsafe { rigel_system_language() } {
            1 => "pt_BR",
            2 => "es",
            _ => "en",
        },
    }
}

pub fn install(language: &str) {
    let data: &[u8] = match language {
        "pt_BR" => PT_BR,
        "es" => ES,
        _ => &[],
    };
    unsafe { rigel_install_translation(data.as_ptr(), data.len() as i32) };
}

/// A message for QML to translate, which keeps following the language while it is on screen.
pub fn message(text: &str, arguments: &[&str]) -> String {
    let mut message = text.to_owned();
    for argument in arguments {
        message.push(ARGUMENT_SEPARATOR);
        message.push_str(argument);
    }
    message
}
