use std::path::Path;

use url::Url;
use crate::download::{DownloadClient, playlist, video};
use crate::options::Options;

async fn find_video_or_playlist(url: &url::Url) -> Result<Url, Box<dyn std::error::Error>> {
    let download_client = DownloadClient::new();

    let html = match download_client.download(url).await {
        Ok(html) => String::from_utf8(html.to_vec()).unwrap(),
        Err(err) => {
            eprintln!("Error downloading html: {}", err);
            return Err(err);
        }
    };

    let get_string_around_index = |index: usize| -> String {
        let is_quote = |char: char| char == '\'' || char == '\"';
        let start = html[..index].rfind(is_quote).unwrap() + 1;
        let end = index + html[index..].find(is_quote).unwrap();
        html[start..end].to_string()
    };

    let video_url = match html.find(".m3u8") {
        Some(index) => {
            println!("Found playlist url in page");
            get_string_around_index(index)
        }
        None => {
            println!("No playlist url found in page searching for video");
            match html.find(".mp4") {
                Some(index) => {
                    println!("Found video url in page");
                    get_string_around_index(index)
                }
                None => {
                    eprintln!("No video or playlist found in page");
                    return Err("No video or playlist found".into());
                }
            }
        }
    };

    Ok(Url::parse(&video_url).unwrap())
}

async fn download_video(url: &Url, output: &Path, options: &Options) -> Result<(), Box<dyn std::error::Error>> {
    let path = url.path();
    let file_extension = path.split('.').last().unwrap_or("");
    match file_extension {
        "mp4" => {
            println!("Downloading mp4 file");
            match video::download_video(url, output, options).await {
                Ok(_) => {}
                Err(err) => {
                    eprintln!("Error downloading file: {}", err);
                    return Err(err);
                }
            }
        }
        "m3u8" => {
            println!("Downloading playlist file");
            match playlist::download_playlist(url, output, options).await {
                Ok(_) => {}
                Err(err) => {
                    eprintln!("Error downloading playlist: {}", err);
                    return Err(err);
                }
            }
        }
        _ => {
            eprintln!("Unsupported file extension: {}", file_extension);
            return Err(Box::new(crate::error::extension_error::ExtensionError));
        }
    }

    Ok(())
}

pub async fn download(url: &str, output: &Path, options: &Options) -> Result<(), Box<dyn std::error::Error>> {
    if std::path::Path::new(output).exists() {
        eprintln!("File already exists: {}", output.to_string_lossy());
        return Err("File already exists".into());
    }


    println!("Downloading {} from: {}", output.to_string_lossy(), url);

    let parsed_url = match url::Url::parse(url) {
        Ok(url) => url,
        Err(e) => {
            eprintln!("Error parsing url: {}", e);
            return Err(Box::new(e));
        }
    };

    match download_video(&parsed_url, output, options).await {
        Ok(_) => {}
        Err(ref err) if err.is::<crate::error::extension_error::ExtensionError>() => {
            println!("Trying to find a video or playlist file in page");
            match find_video_or_playlist(&parsed_url).await {
                Ok(video_url) => {
                    match download_video(&video_url, &output, options).await {
                        Ok(_) => {}
                        Err(err) => {
                            eprintln!("Error downloading video or playlist: {}", err);
                            return Err(err);
                        }
                    }
                }
                Err(err) => {
                    eprintln!("Error finding video or playlist: {}", err);
                    return Err(err);
                }
            }
        }
        Err(err) => {
            return Err(err);
        }
    }


    println!("Finished downloading {} from: {}", output.to_string_lossy(), url);


    // now we have the final file, but we should use ffmpeg to convert it to a playable format
    println!("Converting file to mp4");

    let outfile_name = output.to_str().unwrap();
    let ffmpeg_result = std::process::Command::new("ffmpeg")
        .args(&[
            "-loglevel",
            "error",
            "-i",
            outfile_name,
            "-c",
            "copy",
            (outfile_name.to_string() + ".mp4").as_ref(),
        ])
        .output();


    if let Err(err) = ffmpeg_result {
        eprintln!("Error converting file: {}", err);
        return Err(Box::new(err));
    }

    // remove the original output file and move the ffmpeg output to the original output file
    if let Err(err) = std::fs::remove_file(output) {
        eprintln!("Error removing file: {}", err);
        return Err(Box::new(err));
    }

    if let Err(err) = std::fs::rename(outfile_name.to_string() + ".mp4", outfile_name) {
        eprintln!("Error renaming file: {}", err);
        return Err(Box::new(err));
    }

    Ok(())
}
