/// Double-quoted shell literal; only an explicitly requested ROM placeholder is expanded.
pub fn shell_argument(value: &str, expand_rom: bool) -> Result<String, &'static str> {
    if value.contains('\0') {
        return Err("Shell arguments cannot contain NUL");
    }
    let escaped = value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('$', "\\$")
        .replace('`', "\\`");
    let inner = if expand_rom {
        escaped.replace("{ROM}", "${ROM_PATH}")
    } else {
        escaped
    };
    Ok(format!("\"{inner}\""))
}
