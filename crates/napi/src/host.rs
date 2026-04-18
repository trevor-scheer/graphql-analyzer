use std::path::PathBuf;
use std::sync::OnceLock;

use parking_lot::Mutex;

use graphql_ide::AnalysisHost;

pub struct NapiAnalysisHost {
    pub(crate) host: AnalysisHost,
    pub(crate) schema_files: Vec<PathBuf>,
    pub(crate) document_files: Vec<PathBuf>,
    pub(crate) initialized: bool,
}

static HOST: OnceLock<Mutex<NapiAnalysisHost>> = OnceLock::new();

pub fn get_host() -> &'static Mutex<NapiAnalysisHost> {
    HOST.get_or_init(|| {
        Mutex::new(NapiAnalysisHost {
            host: AnalysisHost::new(),
            schema_files: Vec::new(),
            document_files: Vec::new(),
            initialized: false,
        })
    })
}
