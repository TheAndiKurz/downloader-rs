use std::path::Path;

use crate::{download, options::Options};
use serde::Deserialize;

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct DownloadEntity {
    url: String,
    output: String,
}

pub async fn download_file(file: &str, options: &Options) -> Result<(), Box<dyn std::error::Error>> {
    let file = match std::fs::File::open(file) {
        Ok(file) => file,
        Err(err) => {
            eprintln!("Error opening file ({}): {}", file, err);
            return Err(Box::new(err));
        }
    };
    let reader = std::io::BufReader::new(file);
    let downloads: Vec<DownloadEntity> = match serde_json::from_reader(reader) {
        Ok(downloads) => downloads,
        Err(err) => {
            eprintln!("Error parsing JSON: {}", err);
            return Err(Box::new(err));
        }
    };

    for download in downloads {
        println!();

        if Path::new(&download.output).exists() {
            println!("File {} already exists, therefore skipping download", download.output);
            continue;
        }

        println!("Downloading {} to {}", download.url, download.output);

        match download::search::download(&download.url, &download.output, options).await {
            Ok(_) => {
                println!("Finished downloading {} to {}", download.url, download.output);
                println!();
            },
            Err(err) => {
                eprintln!("Error downloading {}: {}", download.url, err);
                eprintln!();
            }
        }
    }

    Ok(())
}


