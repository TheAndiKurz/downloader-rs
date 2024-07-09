pub mod search;
pub mod playlist;
pub mod video;


use bytes::Bytes;
use reqwest::header::{HeaderMap, CONTENT_LENGTH};
use url::Url;

pub struct DownloadClient {
    client: reqwest::Client,
}


impl DownloadClient {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .user_agent("Mozilla/5.0 (Windows NT 10.0; rv:78.0) Gecko/20100101 Firefox/78.0")
            .build()
            .unwrap();

        Self { client }
    }

    async fn head(&self, url: &Url) -> Result<HeaderMap, Box<dyn std::error::Error>> {
        let request = self.client.head(url.as_str());

        let response = match request.send().await {
            Ok(response) => response,
            Err(err) => {
                eprintln!("Error downloading {}: {}", url, err);
                return Err(Box::new(err));
            }
        };

        match response.error_for_status_ref() {
            Ok(_) => {}
            Err(err) => {
                eprintln!("Error downloading {}: {}", url, err);
                return Err(Box::new(err));
            }
        }

        Ok(response.headers().clone())
    }

    pub async fn get_content_length(&self, url: &Url) -> Result<u64, Box<dyn std::error::Error>> {
        let headers = self.head(url).await?;
        let content_length = match headers.get(CONTENT_LENGTH) {
            Some(header_value) => header_value.to_str().unwrap_or_default().parse::<u64>()?,
            None => {
                eprintln!("Response does not have the content length even if it is a video response");
                return Err("Need content length in header to download video file".into());
            }
        };

        Ok(content_length)
    }

    pub async fn download(&self, url: &Url) -> Result<Bytes, Box<dyn std::error::Error + Send>> {
        self.download_header(url, &HeaderMap::new()).await
    }

    pub async fn download_header(&self, url: &Url, headers: &HeaderMap) -> Result<Bytes, Box<dyn std::error::Error + Send>> {
        let request = self.client.get(url.as_str()).headers(headers.to_owned());

        let response = match request.send().await {
            Ok(response) => response,
            Err(err) => {
                eprintln!("Error downloading {}: {}", url, err);
                return Err(Box::new(err));
            }
        };

        match response.error_for_status_ref() {
            Ok(_) => {}
            Err(err) => {
                eprintln!("Error downloading {}: {}", url, err);
                return Err(Box::new(err));
            }
        }

        let bytes = match response.bytes().await {
            Ok(bytes) => bytes,
            Err(err) => {
                eprintln!("Error reading response: {}", err);
                return Err(Box::new(err));
            }
        };

        Ok(bytes)
    }
}
