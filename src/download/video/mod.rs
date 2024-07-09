pub mod range;

use std::path::Path;
use url::Url;

use crate::options::Options;

use range::{SegmentedVideo, Video};

pub async fn download_video(url: &Url, output: &Path, options: &Options) -> Result<(), Box<dyn std::error::Error>> {
    let video = Video::new(url.clone(), output.to_string_lossy().to_string()).await?;

    let folder = output.parent()
          .unwrap().join(
              output.file_name().unwrap()
                    .to_string_lossy().to_string() 
                    + "_segments"
              )
          .to_owned();

    let mut video_segments = SegmentedVideo::new(video, options.block_size, folder);

    video_segments.download(options).await?;

    video_segments.combine()?;

    Ok(())
}
