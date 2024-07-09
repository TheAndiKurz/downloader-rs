use bytes::Bytes;
use url::Url;

use crate::options::Options;

pub async fn download(url: &Url) -> Result<Bytes, Box<dyn std::error::Error + Send>> {
    let response = match reqwest::get(url.as_str()).await {
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

pub async fn download_file(url: &Url, output: &str, _: &Options) -> Result<(), Box<dyn std::error::Error>> {
    let bytes = match download(url).await {
        Ok(bytes) => bytes,
        Err(err) => {
            eprintln!("Error downloading file: {}", err);
            return Err(err);
        }
    };

    let mut file = match std::fs::File::create(output) {
        Ok(file) => file,
        Err(err) => {
            eprintln!("Error creating file: {}", err);
            return Err(Box::new(err));
        }
    };

    let mut content = std::io::Cursor::new(bytes);
    std::io::copy(&mut content, &mut file)?;

    Ok(())
}
