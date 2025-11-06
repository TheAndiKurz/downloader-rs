use futures::lock::Mutex;
use futures::stream::{self, StreamExt};
use std::path::Path;
use std::sync::Arc;
use url::Url;

use crate::download::playlist::Playlist;
use crate::download::DownloadClient;
use crate::options::Options;

#[derive(Debug, Clone)]
pub struct Segment {
    pub name: String,
    pub uri: Url,
    pub duration: f64,
    pub downloaded: bool,
}

struct SegmentDownloadArgs {
    downloaded_duration: Mutex<f64>,
    total_duration: f64,
    downloaded_segments: Mutex<i32>,
    total_segments: i32,
}

impl Segment {
    async fn finished(&mut self, args: &SegmentDownloadArgs) {
        let mut downloaded_duration = args.downloaded_duration.lock().await;
        let mut downloaded_segments = args.downloaded_segments.lock().await;
        *downloaded_segments += 1;
        *downloaded_duration += self.duration;

        print_time(*downloaded_duration);
        print!(" / ");
        print_time(args.total_duration);
        print!(
            " ({:5.2}%)",
            (*downloaded_duration / args.total_duration) * 100.0
        );

        print!(
            "\t {:width$} / {:width$} segs ({:5.2}%)",
            *downloaded_segments,
            args.total_segments,
            (*downloaded_segments as f64 / args.total_segments as f64) * 100.0,
            width = args.total_segments.to_string().len()
        );

        print!("\t {}", self.name);
        println!();

        drop(downloaded_segments);
        drop(downloaded_duration);
    }

    async fn download(
        &mut self,
        folder_name: &Path,
        client: Arc<DownloadClient>,
    ) -> Result<(), Box<dyn std::error::Error + Send>> {
        if self.downloaded {
            return Ok(());
        }

        let seg_path = folder_name.join(&self.name);
        if seg_path.exists() {
            self.downloaded = true;
            return Ok(());
        }

        let bytes = match client.download(&self.uri).await {
            Ok(bytes) => bytes,
            Err(err) => {
                eprintln!("Error downloading segment: {}", err);
                return Err(err);
            }
        };

        let mut file = match std::fs::File::create(seg_path) {
            Ok(file) => file,
            Err(err) => {
                eprintln!("Error creating file: {}", err);
                return Err(Box::new(err));
            }
        };

        let mut content = std::io::Cursor::new(bytes);
        match std::io::copy(&mut content, &mut file) {
            Ok(_) => {}
            Err(err) => {
                eprintln!("Error writing to file: {}", err);
                return Err(Box::new(err));
            }
        }

        self.downloaded = true;

        Ok(())
    }
}

fn print_time(seconds: f64) {
    let hours = seconds as i64 / 3600;
    let minutes = (seconds as i64 % 3600) / 60;
    let seconds = seconds as i64 % 60;

    print!("{:02}:{:02}:{:02}", hours, minutes, seconds);
}

pub async fn parse_segments(
    playlist: &str,
    prefix: &str,
) -> Result<Vec<Segment>, Box<dyn std::error::Error>> {
    let mut segments = Vec::new();
    let lines = playlist.lines().collect::<Vec<&str>>();

    lines.iter().enumerate().for_each(|(i, line)| {
        if line.starts_with("#EXTINF") {
            let idx_start = line.find(":").unwrap();
            let idx_end = line.find(",").unwrap();
            let duration = line[idx_start + 1..idx_end].parse::<f64>().unwrap();
            let uri = lines[i + 1];
            let uri = match Url::parse(uri) {
                Ok(uri) => uri,
                Err(_) => Url::parse((prefix.to_string() + uri).as_str()).unwrap(),
            };
            segments.push(Segment {
                name: match uri.path().rsplit_once("/") {
                    Some((_, name)) => name.to_string(),
                    None => uri.path().to_string(),
                },
                uri,
                duration,
                downloaded: false,
            });
        }
    });

    Ok(segments)
}

pub async fn download_segments<'a>(
    playlist: &Playlist,
    segment_folder: &'a Path,
    options: &Options,
) -> Result<(), Box<dyn std::error::Error>> {
    let downloaded_duration = Mutex::new(0.0 as f64);
    let downloaded_segments = Mutex::new(0 as i32);
    let http_client = Arc::new(DownloadClient::new());

    let mut segments = playlist.segments.to_owned();

    let args = SegmentDownloadArgs {
        downloaded_duration: downloaded_duration,
        total_duration: playlist.total_duration,
        downloaded_segments: downloaded_segments,
        total_segments: playlist.segments.len() as i32,
    };

    let tasks = segments
        .iter_mut()
        .map(|segment| {
            let http_client = Arc::clone(&http_client);
            async {
                if let Err(err) = segment.download(segment_folder, http_client).await {
                    return Err(err);
                }

                if segment.downloaded {
                    segment.finished(&args).await;
                }

                Ok(segment)
            }
        })
        .collect::<Vec<_>>();

    let results = stream::iter(tasks)
        .buffer_unordered(options.max_parallel_downloads)
        .collect::<Vec<_>>()
        .await;
    for result in results {
        match result {
            Err(err) => {
                eprintln!("Error downloading segment: {}", err);
            }
            _ => {}
        }
    }

    if segments.len() > 0 {
        println!("Retrying {} segments", segments.len());
    }

    Ok(())
}
