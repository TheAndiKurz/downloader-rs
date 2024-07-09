mod file;
mod download;
mod options;
mod error;

use clap::{Subcommand, Parser};

#[derive(Parser, Debug)]
#[command(version, about)]
struct Args {
    #[clap(subcommand)]
    subcmd: SubCmd,
    
    #[clap(short, long, default_value = "4")]
    /// set the maximum number of parallel downloads
    parallel: usize,

    #[clap(short, long, default_value = "3")]
    /// set the maximum number of download retries
    retries: usize,
}

#[derive(Subcommand, Debug)]
#[command(version, about)]
enum SubCmd {
    /// Download files from a json file
    File {
        #[clap(default_value = "download.json")]
        /// provide a formated json file that contains the download links
        file: String, 
    },
    /// Download a single file from a url
    Download {
        #[clap(value_parser = url_parser)]
        /// provide a download link
        url: String,

        /// provide a output file name
        output: String,
    }
}

fn url_parser(url: &str) -> Result<String, String> {
    if url.starts_with("http") {
        Ok(url.to_string())
    } else {
        Err("URL must start with http or https".to_string())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    let options = options::Options {
        max_parallel_downloads: args.parallel,
        max_download_retries: args.retries
    };

    println!("Options: {:?}", options);

    match args.subcmd {
        SubCmd::File { file } => {
            if let Ok(_) = file::download_file(&file, &options).await {
                println!("Finished reading file {}", file);
            }
        }
        SubCmd::Download { url, output } => {
            if let Ok(_) = download::search::download(&url, &output, &options).await {
                println!("Finished downloading {} from: {}", output, url);
            }
        }
    }

    Ok(())
}
