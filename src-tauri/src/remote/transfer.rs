//! Streaming a [`Download`] to a destination, with progress and cancellation.
//!
//! This is the building block of the ROM manager downloads: it copies chunk by chunk
//! (never the whole file in memory), reports the bytes transferred, stops
//! when a cancellation flag is set and detects truncated streams. Choosing
//! the destination (a temporary file), verifying and finalizing it belong to
//! the ROM manager.

use std::sync::atomic::{AtomicBool, Ordering};

use tokio::io::{AsyncReadExt, AsyncWrite, AsyncWriteExt};

use super::provider::{Download, RemoteError};

const CHUNK_SIZE: usize = 256 * 1024;

/// Copies a download into `writer` and returns the number of bytes copied.
/// `on_progress` receives the total bytes copied so far after each chunk
/// (offset included). A stream ending before the announced size is reported
/// as [`RemoteError::ConnectionLost`].
pub async fn copy_download<W>(
    mut download: Download,
    writer: &mut W,
    cancel: &AtomicBool,
    mut on_progress: impl FnMut(u64),
) -> Result<u64, RemoteError>
where
    W: AsyncWrite + Unpin,
{
    let mut buffer = vec![0; CHUNK_SIZE];
    let mut position = download.offset;
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(RemoteError::Cancelled);
        }
        let read = download
            .reader
            .read(&mut buffer)
            .await
            .map_err(|error| RemoteError::from_stream(&error))?;
        if read == 0 {
            break;
        }
        writer
            .write_all(&buffer[..read])
            .await
            .map_err(|error| RemoteError::LocalIo {
                message: error.to_string(),
            })?;
        position += read as u64;
        on_progress(position);
    }
    if download.size.is_some_and(|size| position < size) {
        return Err(RemoteError::ConnectionLost);
    }
    writer.flush().await.map_err(|error| RemoteError::LocalIo {
        message: error.to_string(),
    })?;
    Ok(position - download.offset)
}

#[cfg(test)]
mod tests {
    use std::io;
    use std::pin::Pin;
    use std::task::{Context, Poll};

    use tokio::io::{AsyncRead, ReadBuf};

    use super::*;

    fn download(data: &'static [u8], size: Option<u64>) -> Download {
        Download {
            size,
            offset: 0,
            reader: Box::pin(data),
        }
    }

    /// Yields some bytes, then fails.
    struct Broken(bool);

    impl AsyncRead for Broken {
        fn poll_read(
            mut self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            buf: &mut ReadBuf<'_>,
        ) -> Poll<io::Result<()>> {
            if self.0 {
                return Poll::Ready(Err(io::Error::new(io::ErrorKind::BrokenPipe, "gone")));
            }
            self.0 = true;
            buf.put_slice(b"abc");
            Poll::Ready(Ok(()))
        }
    }

    #[tokio::test]
    async fn copies_and_reports_progress() {
        let mut output = Vec::new();
        let mut progress = Vec::new();
        let copied = copy_download(
            download(b"hello world", Some(11)),
            &mut output,
            &AtomicBool::new(false),
            |position| progress.push(position),
        )
        .await
        .expect("copy");
        assert_eq!(copied, 11);
        assert_eq!(output, b"hello world");
        assert_eq!(progress.last(), Some(&11));
    }

    #[tokio::test]
    async fn detects_cancellation_truncation_and_broken_streams() {
        let mut output = Vec::new();
        assert_eq!(
            copy_download(
                download(b"data", None),
                &mut output,
                &AtomicBool::new(true),
                |_| {}
            )
            .await,
            Err(RemoteError::Cancelled)
        );
        assert_eq!(
            copy_download(
                download(b"short", Some(100)),
                &mut output,
                &AtomicBool::new(false),
                |_| {}
            )
            .await,
            Err(RemoteError::ConnectionLost)
        );
        let broken = Download {
            size: None,
            offset: 0,
            reader: Box::pin(Broken(false)),
        };
        assert_eq!(
            copy_download(broken, &mut output, &AtomicBool::new(false), |_| {}).await,
            Err(RemoteError::ConnectionLost)
        );
    }
}
