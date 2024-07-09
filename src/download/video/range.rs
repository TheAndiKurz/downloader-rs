use std::{io::Cursor, path::PathBuf, sync::Arc};

use reqwest::header::{HeaderMap, RANGE};
use tokio::sync::Mutex;
use url::Url;

use crate::{download::DownloadClient, options::Options};


pub struct Video {
    download_client: DownloadClient,
    url: Url,
    title: String,
    size: u64,
}

impl Video {
    pub async fn new(url: Url, title: String) -> Result<Self, Box<dyn std::error::Error>> {
        let client = DownloadClient::new();

        let size = client.get_content_length(&url).await?;

        let video = Self { 
            download_client: client,
            url, 
            title,
            size
        };

        Ok(video)
    }

}

#[derive(Clone)]
pub struct VideoSegment {
    video: Arc<Video>,
    id: u64,
    start: u64,
    end: u64,
}

impl VideoSegment {
    pub fn new(id: u64, video: Arc<Video>, start: u64, end: u64) -> Self {
        Self { id, video, start, end }
    }

    pub fn size(&self) -> u64 {
        self.end - self.start
    }
    
    pub async fn download(&self, folder: Arc<PathBuf>) -> Result<(), Box<dyn std::error::Error + Send>> {
        let seg_path = folder.join(format!("{}.ts", self.id));

        if seg_path.exists() {
            return Ok(());
        }

        let mut headers = HeaderMap::new();
        headers.insert(RANGE, 
            match format!("bytes={}-{}", self.start, self.end).try_into() {
                Ok(r) => r,
                Err(e) => return Err(Box::new(e)),
            });

        let response = self.video.download_client.download_header(&self.video.url, &headers).await?;

        let mut content = Cursor::new(response);
        let mut file = match std::fs::File::create(seg_path) {
            Ok(f) => f,
            Err(e) => return Err(Box::new(e)),
        };

        std::io::copy(&mut content, &mut file).unwrap();

        Ok(())
    }
}


pub struct SegmentedVideo {
    video: Arc<Video>,
    segments: Vec<VideoSegment>,
    total_segments: u64,
    folder: PathBuf,
}

impl SegmentedVideo {
    pub fn new(video: Video, block_size: u64, folder: PathBuf) -> Self {
        let mut segments = vec![];

        let mut start = 0;
        let mut end = block_size;

        let video = Arc::new(video);

        while end < video.size {
            segments.push(VideoSegment::new(segments.len() as u64, Arc::clone(&video), start, end));
            start = end + 1;
            end = start + block_size;
        }

        segments.push(VideoSegment::new(segments.len() as u64, Arc::clone(&video), start, video.size));

        let total_segments = segments.len() as u64;

        Self { video, segments, folder, total_segments }
    }

    pub async fn download(&mut self, options: &Options) -> Result<(), Box<dyn std::error::Error>> {
        let segment_folder = self.folder.to_owned();

        if !segment_folder.exists() {
            match std::fs::create_dir(&segment_folder) {
                Ok(_) => {}
                Err(err) => {
                    eprintln!("Error creating folder: {}", err);
                    return Err(Box::new(err));
                }
            }
        }

        let segment_folder = Arc::new(segment_folder);
        let semaphore = Arc::new(tokio::sync::Semaphore::new(options.max_parallel_downloads));

        let segments_downloaded = Arc::new(Mutex::new(0));
        let total_segments = Arc::new(self.total_segments);

        let tasks = self.segments.to_owned().into_iter().map(|segment| {
            let folder = Arc::clone(&segment_folder);
            let semaphore = Arc::clone(&semaphore);
            let segments_downloaded = Arc::clone(&segments_downloaded);
            let total_segments = Arc::clone(&total_segments);
            tokio::spawn(async move {
                let _permit = semaphore.acquire().await.unwrap();
                if let Err(err) = segment.download(folder).await {
                    return Err(err);
                }

                let mut segments_downloaded = segments_downloaded.lock().await;
                *segments_downloaded += 1;

                println!("Downloaded {:width$} / {:width$} segments ({:5.2}%)\t ({})",
                    *segments_downloaded,
                    total_segments,
                    (*segments_downloaded as f64 / *total_segments as f64) * 100.,
                    segment.id,
                    width = total_segments.to_string().len());


                Ok(segment)
            })
        }).collect::<Vec<_>>();

        self.segments.clear();

        for task in tasks {
            match task.await {
                Ok(Ok(segment)) => {
                    self.segments.push(segment);
                },
                Ok(Err(err)) => {
                    eprintln!("Error downloading segment: {}", err);
                    return Err(err);
                },
                Err(err) => {
                    eprintln!("Error waiting for task: {}", err);
                    return Err(Box::new(err));
                }
            }
        }

        self.segments.sort_by(|a, b| a.id.cmp(&b.id));

        Ok(())
    }

    pub fn combine(&self) -> Result<(), Box<dyn std::error::Error>> {
        if self.segments.len() != self.total_segments as usize {
            return Err(Box::new(std::io::Error::new(std::io::ErrorKind::Other, "Not all segments downloaded")));
        }

        let mut segments = self.segments.to_owned();
        segments.sort_by(|a, b| a.id.cmp(&b.id));

        let mut file = std::fs::File::create(self.video.title.to_owned())?;

        for segment in segments {
            let seg_path = self.folder.join(format!("{}.ts", segment.id));
            let mut seg_file = std::fs::File::open(seg_path)?;

            std::io::copy(&mut seg_file, &mut file)?;
        }

        Ok(())
    }
}
