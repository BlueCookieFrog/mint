pub mod file;
pub mod http;
pub mod modio;
#[macro_use]
pub mod cache;
pub mod mod_store;

use thiserror::Error;
use tokio::sync::mpsc::Sender;

use std::collections::HashMap;
use std::io::{Read, Seek};
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

pub use cache::*;
pub use mint_lib::mod_info::*;
pub use mod_store::*;

use self::modio::DrgModioError;

type Providers = RwLock<HashMap<&'static str, Arc<dyn ModProvider>>>;

pub trait ReadSeek: Read + Seek + Send {}
impl<T: Seek + Read + Send> ReadSeek for T {}

#[derive(Debug)]
pub enum FetchProgress {
    Progress {
        resolution: ModResolution,
        progress: u64,
        size: u64,
    },
    Complete {
        resolution: ModResolution,
    },
}

impl FetchProgress {
    pub fn resolution(&self) -> &ModResolution {
        match self {
            FetchProgress::Progress { resolution, .. } => resolution,
            FetchProgress::Complete { resolution, .. } => resolution,
        }
    }
}

#[async_trait::async_trait]
pub trait ModProvider: Send + Sync {
    async fn resolve_mod(
        &self,
        spec: &ModSpecification,
        update: bool,
        cache: ProviderCache,
    ) -> Result<ModResponse, ProviderError>;
    async fn fetch_mod(
        &self,
        url: &ModResolution,
        update: bool,
        cache: ProviderCache,
        blob_cache: &BlobCache,
        tx: Option<Sender<FetchProgress>>,
    ) -> Result<PathBuf, ProviderError>;
    async fn update_cache(&self, cache: ProviderCache) -> Result<(), ProviderError>;
    /// Check if provider is configured correctly
    async fn check(&self) -> Result<(), ProviderError>;
    fn get_mod_info(&self, spec: &ModSpecification, cache: ProviderCache) -> Option<ModInfo>;
    fn is_pinned(&self, spec: &ModSpecification, cache: ProviderCache) -> bool;
    fn get_version_name(&self, spec: &ModSpecification, cache: ProviderCache) -> Option<String>;
}

#[derive(Debug, Error)]
pub enum ProviderError {
    #[error("failed to initialize provider {id} with parameters {parameters:?}")]
    InitProviderFailed {
        id: &'static str,
        parameters: HashMap<String, String>,
    },
    #[error(transparent)]
    CacheError(#[from] CacheError),
    #[error(transparent)]
    DrgModioError(#[from] DrgModioError),
    #[error("mod.io-related error encountered while working on mod {mod_id}: {source}")]
    ModioErrorWithModCtxt { source: ::modio::Error, mod_id: u32 },
    #[error("I/O error encountered while working on mod {mod_id}: {source}")]
    IoErrorWithModCtxt { source: std::io::Error, mod_id: u32 },
    #[error(transparent)]
    BlobCacheError(#[from] BlobCacheError),
    #[error("could not find mod provider for <{url}>")]
    ProviderNotFound { url: String },
    #[error("no provider found given <{url}> and factory {}", factory.id)]
    NoProvider {
        url: String,
        factory: &'static ProviderFactory,
    },
    #[error("invalid url <{url}>")]
    InvalidUrl { url: String },
    #[error("request for <{url}> failed: {source}")]
    RequestFailed { source: reqwest::Error, url: String },
    #[error("response from <{url}> failed: {source}")]
    ResponseError { source: reqwest::Error, url: String },
    #[error("mime from <{url}> contains non-ascii characters")]
    InvalidMime {
        source: reqwest::header::ToStrError,
        url: String,
    },
    #[error("unexpected content type from <{url}>: {found_content_type}")]
    UnexpectedContentType {
        found_content_type: String,
        url: String,
    },
    #[error("error while fetching mod <{url}>")]
    FetchError { source: reqwest::Error, url: String },
    #[error("error processing <{url}> while writing to local buffer")]
    BufferIoError { source: std::io::Error, url: String },
    #[error("preview mod links cannot be added directly, please subscribe to the mod on mod.io and and then use the non-preview link")]
    PreviewLink { url: String },
    #[error("mod <{url}> does not have an associated modfile")]
    NoAssociatedModfile { url: String },
    #[error("multiple mods returned for name \"{name_id}\"")]
    AmbiguousModNameId { name_id: String },
    #[error("no mods returned for name \"{name_id}\"")]
    NoModsForNameId { name_id: String },
}

impl ProviderError {
    pub fn opt_mod_id(&self) -> Option<u32> {
        match self {
            ProviderError::DrgModioError(source) => source.opt_mod_id(),
            ProviderError::ModioErrorWithModCtxt { mod_id, .. }
            | ProviderError::IoErrorWithModCtxt { mod_id, .. } => Some(*mod_id),
            _ => None,
        }
    }
}

#[derive(Clone)]
pub struct ProviderFactory {
    pub id: &'static str,
    #[allow(clippy::type_complexity)]
    new: fn(&HashMap<String, String>) -> Result<Arc<dyn ModProvider>, ProviderError>,
    can_provide: fn(&str) -> bool,
    pub parameters: &'static [ProviderParameter<'static>],
}

impl std::fmt::Debug for ProviderFactory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProviderFactory")
            .field("id", &self.id)
            .field("parameters", &self.parameters)
            .finish()
    }
}

#[derive(Debug, Clone)]
pub struct ProviderParameter<'a> {
    pub id: &'a str,
    pub name: &'a str,
    pub description: &'a str,
    pub link: Option<&'a str>,
}

inventory::collect!(ProviderFactory);
