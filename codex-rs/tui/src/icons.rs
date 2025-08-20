/// Icons used throughout the TUI. We fall back to ASCII-safe variants
/// when runtime detection shows emoji cell widths are inconsistent with
/// what we use for wrapping.
use crate::emoji_width::emojis_render_as_expected;

pub fn running() -> &'static str {
    if emojis_render_as_expected() {
        "⚡ Running"
    } else {
        "> Running"
    }
}

pub fn working() -> &'static str {
    if emojis_render_as_expected() {
        "⚙︎ Working"
    } else {
        "* Working"
    }
}

pub fn completed_label() -> &'static str {
    if emojis_render_as_expected() {
        "✓"
    } else {
        "OK"
    }
}

pub fn failed_label() -> &'static str {
    if emojis_render_as_expected() {
        "✗"
    } else {
        "X"
    }
}

pub fn folder() -> &'static str {
    if emojis_render_as_expected() {
        "📂"
    } else {
        "[dir]"
    }
}

pub fn book() -> &'static str {
    if emojis_render_as_expected() {
        "📖"
    } else {
        "[read]"
    }
}

pub fn search() -> &'static str {
    if emojis_render_as_expected() {
        "🔎"
    } else {
        "[find]"
    }
}

pub fn formatting() -> &'static str {
    if emojis_render_as_expected() {
        "✨"
    } else {
        "[fmt]"
    }
}

pub fn test() -> &'static str {
    if emojis_render_as_expected() {
        "🧪"
    } else {
        "[test]"
    }
}

pub fn lint() -> &'static str {
    if emojis_render_as_expected() {
        "🧹"
    } else {
        "[lint]"
    }
}

pub fn keyboard_cmd() -> &'static str {
    if emojis_render_as_expected() {
        "⌨︎"
    } else {
        "[cmd]"
    }
}

pub fn noop() -> &'static str {
    if emojis_render_as_expected() {
        "🔄"
    } else {
        "[noop]"
    }
}

pub fn clipboard() -> &'static str {
    if emojis_render_as_expected() {
        "📋"
    } else {
        "[plan]"
    }
}

pub fn apply_patch() -> &'static str {
    if emojis_render_as_expected() {
        "✏︎ Applying patch"
    } else {
        "Applying patch"
    }
}

pub fn workspace() -> &'static str {
    if emojis_render_as_expected() {
        "📂"
    } else {
        "[dir]"
    }
}

pub fn account() -> &'static str {
    if emojis_render_as_expected() {
        "👤"
    } else {
        "[acct]"
    }
}

pub fn model() -> &'static str {
    if emojis_render_as_expected() {
        "🧠"
    } else {
        "[model]"
    }
}

pub fn token_usage() -> &'static str {
    if emojis_render_as_expected() {
        "📊"
    } else {
        "[tokens]"
    }
}

pub fn wave_error() -> &'static str {
    if emojis_render_as_expected() {
        "🖐"
    } else {
        "[!]"
    }
}
