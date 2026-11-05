/// Formats an editable argument field without losing empty arguments or separators.
pub fn format_arguments(arguments: &[String]) -> String {
    arguments
        .iter()
        .map(|arg| {
            if arg.is_empty()
                || arg
                    .chars()
                    .any(|c| c.is_whitespace() || c == '"' || c == '\\')
            {
                format!("\"{}\"", arg.replace('\\', "\\\\").replace('"', "\\\""))
            } else {
                arg.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Returns `None` for an unclosed quote or escape.
pub fn parse_arguments(text: &str) -> Option<Vec<String>> {
    let (mut result, mut current) = (Vec::new(), String::new());
    let (mut quoted, mut escaped, mut started) = (false, false, false);
    for c in text.chars() {
        if escaped {
            if c != '"' && c != '\\' {
                current.push('\\');
            }
            current.push(c);
            escaped = false;
        } else if quoted && c == '\\' {
            escaped = true;
        } else if c == '"' {
            quoted = !quoted;
            started = true;
        } else if c.is_whitespace() && !quoted {
            if started {
                result.push(std::mem::take(&mut current));
                started = false;
            }
        } else {
            current.push(c);
            started = true;
        }
    }
    if quoted || escaped {
        return None;
    }
    if started {
        result.push(current);
    }
    Some(result)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GameLaunchRequest {
    pub entry: crate::library::GameEntry,
    pub rom_path: Option<String>,
    pub steam_app_id: Option<i32>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GameResult {
    pub exit_code: i32,
    pub duration_ms: i64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PickerType {
    File,
    Directory,
    Any,
}
pub trait GameLauncher {
    type Error;
    fn launch(
        &self,
        request: GameLaunchRequest,
    ) -> impl std::future::Future<Output = Result<GameResult, Self::Error>> + Send;
}
pub trait BrowserLauncher {
    type Error;
    fn open(&self, url: &str) -> Result<(), Self::Error>;
}
pub trait PathPicker {
    type Error;
    fn pick(
        &self,
        label: &str,
        initial_path: &str,
        kind: PickerType,
    ) -> Result<Option<String>, Self::Error>;
}
pub trait SteamLaunchPreparation {
    type Error;
    fn prepare(&self, entry: &crate::library::Emulator, rom_path: &str) -> Result<(), Self::Error>;
}
