use std::future::Future;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpdateInfo {
    pub version: String,
    pub tag_name: String,
    pub release_notes: String,
    pub download_url: String,
    pub asset_size: i64,
    pub asset_name: String,
}
pub trait UpdateFiles {
    type Error;
    fn prepare_destination(
        &self,
        info: &UpdateInfo,
    ) -> impl Future<Output = Result<String, Self::Error>> + Send;
    fn size(&self, path: &str) -> Result<u64, Self::Error>;
    fn delete(&self, path: &str) -> Result<(), Self::Error>;
}
pub trait UpdateInstaller {
    type Error;
    fn is_steam_game_mode(&self) -> Result<bool, Self::Error>;
    fn restart(&self, path: &str) -> Result<bool, Self::Error>;
}
pub trait UpdateSource {
    type Error;
    fn check(&self) -> impl Future<Output = Result<Option<UpdateInfo>, Self::Error>> + Send;
    fn download(
        &self,
        info: &UpdateInfo,
        destination: &str,
        on_progress: impl FnMut(f32) + Send,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;
}
