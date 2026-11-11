pub(crate) mod atomic;
pub mod env;
pub mod session;
pub mod shell;

pub use env::AppImageEnvironment;
pub use session::{gamescope_session_from, is_gamescope_session};
pub use shell::shell_argument;
