use std::sync::Arc;

use tokio::sync::Mutex;
use url::Url;

use crate::{download::download::download, options::Options};

#[derive(Debug)]
struct Playlist {
    total_duration: f64,
    segments: Vec<Segment>,
}

#[derive(Debug, Clone)]
struct Segment {
    name: String,
    uri: Url,
    duration: f64,
    downloaded: bool,
}

struct Stream {
    playlist_url: Url,
    bandwidth: i64,
}

struct SegmentDownloadArgs {
    downloaded_duration: Arc<Mutex<f64>>,
    total_duration: f64,
    downloaded_segments: Arc<Mutex<i32>>,
    total_segments: i32,
    semaphore: Arc<tokio::sync::Semaphore>,
}

impl Segment {
    async fn finished(&mut self, args: &SegmentDownloadArgs) {
        let mut downloaded_duration = args.downloaded_duration.lock().await;
        let mut downloaded_segments = args.downloaded_segments.lock().await;
        *downloaded_segments += 1;
        *downloaded_duration += self.duration;

        print!("Downloaded:  {}/{}s ({:.2}%)",
            *downloaded_duration,
            args.total_duration,
            (*downloaded_duration / args.total_duration) * 100.0
        );
        println!("\t {}/{} segs ({:.2}%)", 
            *downloaded_segments, 
            args.total_segments, 
            (*downloaded_segments as f64 / args.total_segments as f64) * 100.0
        );

        std::mem::drop(downloaded_segments);
        std::mem::drop(downloaded_duration);
    }


    async fn download(&mut self, folder_name: &str, args: SegmentDownloadArgs) -> Result<(), Box<dyn std::error::Error + Send>> {
        if self.downloaded {
            self.finished(&args).await;
            return Ok(());
        }

        let seg_name = folder_name.to_string() + "/" + &self.name;
        if std::path::Path::new(seg_name.as_str()).exists() {
            self.downloaded = true;
            self.finished(&args).await;
            return Ok(());
        }

        let _permit = args.semaphore.acquire().await.unwrap();

        if !std::path::Path::new(folder_name).exists() {
            match std::fs::create_dir(folder_name) {
                Ok(_) => {}
                Err(err) => {
                    eprintln!("Error creating directory: {}", err);
                    return Err(Box::new(err));
                }
            }
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

        self.finished(&args).await;

        Ok(())
    }
}

async fn parse_playlist_segments(playlist: &str, prefix: &str) -> Result<Playlist, Box<dyn std::error::Error>> {
    let mut segments = Vec::new();
    let lines = playlist.lines().collect::<Vec<&str>>();

    lines.iter().enumerate().for_each(|(i, line)| {
        if line.starts_with("#EXTINF") {
            let idx_start = line.find(":").unwrap();
            let idx_end = line.find(",").unwrap();
            let duration = line[idx_start + 1..idx_end].parse::<f64>().unwrap();
            let uri = lines[i + 1];
            segments.push(Segment {
                name: format!("segment_{}", i),
                uri: match Url::parse(&uri) {
                    Ok(uri) => uri,
                    Err(_) => Url::parse((prefix.to_string() + uri).as_str()).unwrap(),
                },
                duration,
                downloaded: false,
            });
        }
    });


    Ok(Playlist {
        total_duration: segments.iter().map(|segment| segment.duration).sum(),
        segments,
    })
}


fn parse_playlist_master<'a>(playlist: &str, prefix: &str) -> Result<Stream, Box<dyn std::error::Error>> {
    let mut streams = Vec::new();

    let lines = playlist.lines().collect::<Vec<&str>>();

    lines.iter().enumerate().for_each(|(i, line)| {
        if line.starts_with("#EXT-X-STREAM-INF") {
            let search = "BANDWIDTH=";
            let idx_start = line.find(search).unwrap();
            let idx_end = idx_start + line[idx_start..].find(",").unwrap();
            let bandwidth = line[idx_start + 1 + search.len()..idx_end].parse::<i64>().unwrap();
            let uri = lines[i + 1];
            streams.push(Stream {
                playlist_url: match Url::parse(uri) {
                    Ok(uri) => uri,
                    Err(_) => Url::parse((prefix.to_string() + uri).as_str()).unwrap(),
                },
                bandwidth,
            });
        }
    });

    let selected_stream = streams.into_iter().max_by_key(|stream| stream.bandwidth).unwrap();

    Ok(selected_stream)
}


async fn parse_playlist(playlist_url: &Url) -> Result<Playlist, Box<dyn std::error::Error>> {
    let playlist = match download(playlist_url).await {
        Ok(playlist) => match String::from_utf8(playlist.to_vec()) {
            Ok(playlist) => playlist,
            Err(err) => {
                eprintln!("Error parsing playlist: {}", err);
                return Err(Box::new(err));
            }
        },
        Err(err) => {
            eprintln!("Error downloading playlist: {}", err);
            return Err(err);
        }
    };

    let prefix = playlist_url.as_str().rsplit_once("/").unwrap().0.to_string() + "/";

    match playlist.find("#EXT-X-STREAM-INF") {
        Some(_) => {
            let stream = match parse_playlist_master(playlist.as_str(), prefix.as_str()) {
                Ok(stream) => stream,
                Err(err) => {
                    eprintln!("Error parsing master playlist: {}", err);
                    return Err(err);
                }
            };

            let playlist = match download(&stream.playlist_url).await {
                Ok(playlist) => match String::from_utf8(playlist.to_vec()) {
                    Ok(playlist) => playlist,
                    Err(err) => {
                        eprintln!("Error parsing playlist: {}", err);
                        return Err(Box::new(err));
                    }
                },
                Err(err) => {
                    eprintln!("Error downloading playlist: {}", err);
                    return Err(err);
                }
            };

            parse_playlist_segments(playlist.as_str(), prefix.as_str()).await
        }
        None => parse_playlist_segments(playlist.as_str(), prefix.as_str()).await
    }
}

pub async fn download_playlist(playlist_url: &Url, output: &str, options: &Options) -> Result<(), Box<dyn std::error::Error>> {
    let playlist = match parse_playlist(playlist_url).await {
        Ok(playlist) => playlist,
        Err(err) => {
            eprintln!("Error parsing playlist: {}", err);
            return Err(err);
        }
    };
    
    let semaphore = Arc::new(tokio::sync::Semaphore::new(options.max_parallel_downloads));
    let downloaded_duration = Arc::new(Mutex::new(0.0 as f64));
    let downloaded_segments = Arc::new(Mutex::new(0 as i32));

    let folder_name = output.to_string() + "_segments";
    let tasks = playlist.segments.to_owned().into_iter().map(
            |mut segment| {
                let folder_name = folder_name.clone();
                let args = SegmentDownloadArgs {
                    downloaded_duration: Arc::clone(&downloaded_duration),
                    total_duration: playlist.total_duration,
                    downloaded_segments: Arc::clone(&downloaded_segments),
                    total_segments: playlist.segments.len() as i32,
                    semaphore: Arc::clone(&semaphore)
                };

                tokio::spawn(async move {
                    segment.download(folder_name.as_str(), args).await
                })
            }
        ).collect::<Vec<_>>();


    for task in tasks {
        match task.await {
            Ok(result) => match result {
                Ok(_) => {}
                Err(err) => {
                    eprintln!("Error downloading segment: {}", err);
                    return Err(err);
                }
            },
            Err(err) => {
                eprintln!("Error downloading segment: {}", err);
                return Err(Box::new(err));
            }
        }
    }

    // check if all segments were downloaded

    let downloaded_segments = downloaded_segments.lock().await;
    if *downloaded_segments != playlist.segments.len() as i32 {
        eprintln!("Error downloading segments: not all segments were downloaded");
        return Err(Box::new(std::io::Error::new(std::io::ErrorKind::Other, "Not all segments were downloaded")));
    }

    // segments are downloaded, now we need to merge them
    
    let mut file = match std::fs::File::create(output) {
        Ok(file) => file,
        Err(err) => {
            eprintln!("Error creating file: {}", err);
            return Err(Box::new(err));
        }
    };

    for segment in playlist.segments {
        let segment_file = match std::fs::File::open(folder_name.clone() + "/" + &segment.name) {
            Ok(file) => file,
            Err(err) => {
                eprintln!("Error opening file: {}", err);
                return Err(Box::new(err));
            }
        };

        let mut content = std::io::BufReader::new(segment_file);
        match std::io::copy(&mut content, &mut file) {
            Ok(_) => {}
            Err(err) => {
                eprintln!("Error writing to file: {}", err);
                return Err(Box::new(err));
            }
        }
    }

    // delete segments folder
    
    match std::fs::remove_dir_all(folder_name) {
        Ok(_) => {}
        Err(err) => {
            eprintln!("Error deleting folder: {}", err);
            return Err(Box::new(err));
        }
    }

    Ok(())
}
