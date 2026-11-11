use crate::util::AppImageEnvironment;
use emusync_domain::execution::BrowserLauncher;
use std::{io, process::Command};

#[derive(Clone, Copy, Debug, Default)]
pub struct DesktopBrowserLauncher;

impl BrowserLauncher for DesktopBrowserLauncher {
    type Error = io::Error;

    fn open(&self, url: &str) -> Result<(), Self::Error> {
        open_browser(url)
    }
}

pub fn open_browser(url: &str) -> io::Result<()> {
    if cfg!(windows) {
        if Command::new("rundll32")
            .args(["url.dll,FileProtocolHandler", url])
            .spawn()
            .is_ok()
        {
            return Ok(());
        }
        let escaped = url.replace('&', "^&");
        Command::new("cmd")
            .args(["/c", "start", "\"\"", &escaped])
            .spawn()
            .map(|_| ())
    } else if cfg!(target_os = "macos") {
        Command::new("open").arg(url).spawn().map(|_| ())
    } else {
        let mut command = Command::new("xdg-open");
        command.arg(url);
        AppImageEnvironment::clean_command(&mut command);
        command.spawn().map(|_| ())
    }
}
