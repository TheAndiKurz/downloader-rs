use url::Url;
use crate::{download::download::download, options::Options};
use crate::download::playlist::segment;
use crate::download::playlist::segment::{parse_segments, Segment};

#[derive(Debug)]
pub struct Playlist {
    pub total_duration: f64,
    pub segments: Vec<Segment>,
}

pub struct Stream {
    playlist_url: Url,
    bandwidth: i64,
}

pub fn parse_playlist_master(playlist: &str, prefix: &str) -> Result<Stream, Box<dyn std::error::Error>> {
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

    let segments = match playlist.find("#EXT-X-STREAM-INF") {
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

            parse_segments(playlist.as_str(), prefix.as_str()).await
        }
        None => parse_segments(playlist.as_str(), prefix.as_str()).await
    }?;

    Ok(Playlist {
        total_duration: segments.iter().map(|segment| segment.duration).sum(),
        segments
    })
}


pub async fn download_playlist(playlist_url: &Url, output: &str, options: &Options) -> Result<(), Box<dyn std::error::Error>> {
    let playlist = match parse_playlist(playlist_url).await {
        Ok(playlist) => playlist,
        Err(err) => {
            eprintln!("Error parsing playlist: {}", err);
            return Err(err);
        }
    };
    
    let folder_name = output.to_string() + "_segments";

    if !std::path::Path::new(folder_name.as_str()).exists() {
        match std::fs::create_dir(folder_name.clone()) {
            Ok(_) => {}
            Err(err) => {
                eprintln!("Error creating folder: {}", err);
                return Err(Box::new(err));
            }
        }
    }
    
    segment::download_segments(&playlist, folder_name.as_str(), options).await?;

    // segments are downloaded, now we need to merge them
    let mut file = match std::fs::File::create(output) {
        Ok(file) => file,
        Err(err) => {
            eprintln!("Error creating file: {}", err);
            return Err(Box::new(err));
        }
    };

    for segment in playlist.segments {
        let seg_name = folder_name.clone() + "/" + &segment.name;
        let segment_file = match std::fs::File::open(seg_name.clone()) {
            Ok(file) => file,
            Err(err) => {
                eprintln!("Error opening file ({}): {}", seg_name, err);
                return Err(Box::new(err));
            }
        };

        let mut content = std::io::BufReader::new(segment_file);
        std::io::copy(&mut content, &mut file)?;
    }

    Ok(())
}
