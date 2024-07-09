use std::sync::Arc;
use tokio::sync::Mutex;
use url::Url;

use crate::download::download::download;
use crate::download::playlist::playlist::Playlist;
use crate::options::Options;

#[derive(Debug, Clone)]
pub struct Segment {
    pub name: String,
    pub uri: Url,
    pub duration: f64,
    pub downloaded: bool,
}


struct SegmentDownloadArgs {
    downloaded_duration: Arc<Mutex<f64>>,
    total_duration: f64,
    downloaded_segments: Arc<Mutex<i32>>,
    total_segments: i32,
}

impl Clone for SegmentDownloadArgs {
    fn clone(&self) -> SegmentDownloadArgs {
        SegmentDownloadArgs {
            downloaded_duration: Arc::clone(&self.downloaded_duration),
            total_duration: self.total_duration,
            downloaded_segments: Arc::clone(&self.downloaded_segments),
            total_segments: self.total_segments,
        }
    }
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
        print!(" ({:5.2}%)", (*downloaded_duration / args.total_duration) * 100.0);

        print!("\t {:width$} / {:width$} segs ({:5.2}%)", 
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


    async fn download(&mut self, folder_name: &str) -> Result<(), Box<dyn std::error::Error + Send>> {
        if self.downloaded {
            return Ok(());
        }

        let seg_name = folder_name.to_string() + "/" + &self.name;
        if std::path::Path::new(seg_name.as_str()).exists() {
            self.downloaded = true;
            return Ok(());
        }


        let bytes = match download(&self.uri).await {
            Ok(bytes) => bytes,
            Err(err) => {
                eprintln!("Error downloading segment: {}", err);
                return Err(err);
            }
        };

        let mut file = match std::fs::File::create(seg_name) {
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

pub async fn parse_segments(playlist: &str, prefix: &str) -> Result<Vec<Segment>, Box<dyn std::error::Error>> {
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

pub async fn download_segments(playlist: &Playlist, folder_name: &str, options: &Options) -> Result<(), Box<dyn std::error::Error>> {
    let semaphore = Arc::new(tokio::sync::Semaphore::new(options.max_parallel_downloads));
    let downloaded_duration = Arc::new(Mutex::new(0.0 as f64));
    let downloaded_segments = Arc::new(Mutex::new(0 as i32));

    let mut segments = playlist.segments.to_owned();

    let args = SegmentDownloadArgs {
        downloaded_duration: Arc::clone(&downloaded_duration),
        total_duration: playlist.total_duration,
        downloaded_segments: Arc::clone(&downloaded_segments),
        total_segments: playlist.segments.len() as i32,
    };

    let mut tries = 0;
    
    while segments.len() > 0 && tries < options.max_download_retries {
        let tasks = segments.into_iter().map(
            |mut segment| {
                let args = args.clone();
                let semaphore = Arc::clone(&semaphore);
                let folder_name = folder_name.to_string();
                tokio::spawn(async move {
                    let permit = semaphore.acquire().await.unwrap();

                    if let Err(err) = segment.download(folder_name.as_str()).await {
                        return Err(err);
                    }

                    std::mem::drop(permit);
                    if segment.downloaded {
                        segment.finished(&args).await;
                    }

                    Ok(segment)
                })
            }
        ).collect::<Vec<_>>();

        segments = Vec::new();

        for task in tasks {
            match task.await {
                Ok(Ok(segment)) if !segment.downloaded => {
                    segments.push(segment);
                },
                Ok(Err(err)) => {
                    eprintln!("Error downloading segment: {}", err);
                },
                Err(err) => {
                    eprintln!("Error waiting for task: {}", err);
                },
                _ => {}
            }
        }

        if segments.len() > 0 {
            println!("Retrying {} segments", segments.len());
        }

        tries += 1;
    }

    Ok(())
}
