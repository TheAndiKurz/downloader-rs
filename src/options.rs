
#[derive(Debug)]
pub struct Options {
    pub max_parallel_downloads: usize,
    pub max_download_retries: usize,
    pub block_size: u64,
}
