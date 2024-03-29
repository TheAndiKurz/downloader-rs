mod playlist;
mod download;

use url::Url;

use crate::options::Options;

async fn find_video_or_playlist(url: &url::Url) -> Result<Url, Box<dyn std::error::Error>> {
    let html = match download::download(url).await {
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

    let video_url = match html.find(".mp4") {
        Some(index) => {
            println!("Found video url in page");
            get_string_around_index(index)
        }
        None => {
            println!("No video url found in page searching for playlist");
            match html.find(".m3u8") {
                Some(index) => {
                    println!("Found playlist url in page");
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

async fn download_video(url: &Url, output: &str, options: &Options) -> Result<(), Box<dyn std::error::Error>> {
    let path = url.path();
    let file_extension = path.split('.').last().unwrap_or("");
    match file_extension {
        "mp4" => {
            println!("Downloading mp4 file");
            match download::download_file(url, output, options).await {
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

pub async fn download(url: &str, output: &str, options: &Options) -> Result<(), Box<dyn std::error::Error>> {
    if std::path::Path::new(output).exists() {
        eprintln!("File already exists: {}", output);
        return Err("File already exists".into());
    }


    println!("Downloading {} from: {}", output, url);

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


    println!("Finished downloading {} from: {}", output, url);

    // now we have the final file, but we should use ffmpeg to convert it to a playable format
    
    println!("Converting file to mp4");

    let ffmpeg_result = std::process::Command::new("ffmpeg")
        .args(&[
            "-loglevel",
            "error",
            "-i",
            output,
            "-c",
            "copy",
            (output.to_string() + ".mp4").as_str(),
        ])
        .output();


    match ffmpeg_result {
        Ok(_) => {}
        Err(err) => {
            eprintln!("Error converting file: {}", err);
            return Err(Box::new(err));
        }
    };

    // remove the original output file and move the ffmpeg output to the original output file
    
    match std::fs::remove_file(output) {
        Ok(_) => {}
        Err(err) => {
            eprintln!("Error removing file: {}", err);
            return Err(Box::new(err));
        }
    }

    match std::fs::rename(output.to_string() + ".mp4", output) {
        Ok(_) => {}
        Err(err) => {
            eprintln!("Error renaming file: {}", err);
            return Err(Box::new(err));
        }
    }

    Ok(())
}
