//! The pipes to the web view helper, kept off the UI thread.
//!
//! A hung helper stops reading its stdin, the pipe fills (about 64 KB) and
//! a plain `write_all` then blocks forever. [`HelperWriter`] puts the write
//! on its own thread behind a bounded queue: when the queue is full the
//! helper is treated as wedged and the message is dropped instead of
//! blocking. [`read_bounded_line`] reads the helper's stdout one line at a
//! time without ever holding more than a capped line.

use std::io::{self, BufRead, Write};
use std::sync::mpsc::{SyncSender, TrySendError, sync_channel};

/// Messages that may wait for the writer thread.
const QUEUE_LEN: usize = 64;

/// What happened to a message handed to [`HelperWriter::send`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Sent {
    Queued,
    /// The queue is full or the writer is gone: the helper is not reading.
    /// The message was dropped.
    Wedged,
}

/// Writes lines to the helper from a background thread.
pub(super) struct HelperWriter {
    queue: Option<SyncSender<Vec<u8>>>,
}

impl HelperWriter {
    pub(super) fn spawn(sink: impl Write + Send + 'static) -> Self {
        Self::spawn_with_capacity(sink, QUEUE_LEN)
    }

    fn spawn_with_capacity(mut sink: impl Write + Send + 'static, capacity: usize) -> Self {
        let (queue, messages) = sync_channel::<Vec<u8>>(capacity);
        let started = quill::thread_guard::spawn_guarded("quill-webview-writer", move || {
            for message in messages {
                if sink
                    .write_all(&message)
                    .and_then(|()| sink.flush())
                    .is_err()
                {
                    break;
                }
            }
        });
        Self {
            queue: started.is_ok().then_some(queue),
        }
    }

    /// Queue `message` without waiting for the helper.
    pub(super) fn send(&self, message: Vec<u8>) -> Sent {
        match self.queue.as_ref().map(|queue| queue.try_send(message)) {
            Some(Ok(())) => Sent::Queued,
            Some(Err(TrySendError::Full(_) | TrySendError::Disconnected(_))) | None => Sent::Wedged,
        }
    }
}

/// One line read by [`read_bounded_line`].
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Line {
    /// A line without its newline, at most `cap` bytes.
    Text(Vec<u8>),
    /// A line longer than `cap`; it was skipped, not stored.
    TooLong,
    /// End of input.
    End,
}

/// Read up to the next newline, keeping at most `cap` bytes. A longer line
/// is consumed from the reader but never accumulated, so a helper that
/// streams endless output cannot make Quill allocate without bound.
pub(super) fn read_bounded_line(reader: &mut impl BufRead, cap: usize) -> io::Result<Line> {
    let mut line = Vec::new();
    let mut too_long = false;
    let mut read_any = false;
    loop {
        let (used, found) = {
            let chunk = reader.fill_buf()?;
            if chunk.is_empty() {
                break;
            }
            read_any = true;
            let newline = chunk.iter().position(|&byte| byte == b'\n');
            let end = newline.unwrap_or(chunk.len());
            if !too_long {
                let room = cap.saturating_sub(line.len());
                match chunk.get(..end.min(room)) {
                    Some(part) if end <= room => line.extend_from_slice(part),
                    _ => too_long = true,
                }
            }
            (newline.map_or(chunk.len(), |at| at + 1), newline.is_some())
        };
        reader.consume(used);
        if found {
            return Ok(finish(line, too_long));
        }
    }
    if !read_any {
        return Ok(Line::End);
    }
    Ok(finish(line, too_long))
}

fn finish(mut line: Vec<u8>, too_long: bool) -> Line {
    if too_long {
        return Line::TooLong;
    }
    if line.last() == Some(&b'\r') {
        line.pop();
    }
    Line::Text(line)
}

#[cfg(test)]
mod tests {
    use super::{HelperWriter, Line, Sent, read_bounded_line};
    use std::io::{self, BufReader, Cursor, Write};
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    /// A pipe that never drains: the first write parks until released.
    struct StuckPipe(mpsc::Receiver<()>);

    impl Write for StuckPipe {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            let _ = self.0.recv_timeout(Duration::from_secs(10));
            Ok(buf.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn a_full_pipe_never_blocks_the_caller() {
        let (release, stuck) = mpsc::channel();
        let writer = HelperWriter::spawn_with_capacity(StuckPipe(stuck), 4);
        let started = Instant::now();
        let results: Vec<Sent> = (0..20).map(|_| writer.send(b"x\n".to_vec())).collect();
        assert!(started.elapsed() < Duration::from_secs(1));
        assert_eq!(results.first(), Some(&Sent::Queued));
        assert_eq!(results.last(), Some(&Sent::Wedged));
        let queued = results.iter().filter(|r| **r == Sent::Queued).count();
        assert!((4..=5).contains(&queued), "queued {queued}");
        let dropping = Instant::now();
        drop(writer);
        assert!(dropping.elapsed() < Duration::from_secs(1));
        let _ = release.send(());
    }

    #[test]
    fn messages_reach_a_healthy_helper_in_order() {
        struct Collect(mpsc::Sender<Vec<u8>>);
        impl Write for Collect {
            fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
                let _ = self.0.send(buf.to_vec());
                Ok(buf.len())
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let (tx, rx) = mpsc::channel();
        let writer = HelperWriter::spawn(Collect(tx));
        assert_eq!(writer.send(b"one\n".to_vec()), Sent::Queued);
        assert_eq!(writer.send(b"two\n".to_vec()), Sent::Queued);
        assert_eq!(
            rx.recv_timeout(Duration::from_secs(5)).ok(),
            Some(b"one\n".to_vec())
        );
        assert_eq!(
            rx.recv_timeout(Duration::from_secs(5)).ok(),
            Some(b"two\n".to_vec())
        );
    }

    #[test]
    fn a_dead_helper_reports_wedged() {
        struct Broken;
        impl Write for Broken {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                Err(io::ErrorKind::BrokenPipe.into())
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let writer = HelperWriter::spawn(Broken);
        let _ = writer.send(b"a\n".to_vec());
        let wedged = (0..200).any(|_| {
            std::thread::sleep(Duration::from_millis(10));
            writer.send(b"b\n".to_vec()) == Sent::Wedged
        });
        assert!(wedged);
    }

    #[test]
    fn lines_are_split_and_a_long_one_is_skipped_without_storing_it() {
        let mut input = Cursor::new(b"ab\r\nabcdefghij\nok\n".to_vec());
        assert_eq!(
            read_bounded_line(&mut input, 4).ok(),
            Some(Line::Text(b"ab".to_vec()))
        );
        assert_eq!(read_bounded_line(&mut input, 4).ok(), Some(Line::TooLong));
        assert_eq!(
            read_bounded_line(&mut input, 4).ok(),
            Some(Line::Text(b"ok".to_vec()))
        );
        assert_eq!(read_bounded_line(&mut input, 4).ok(), Some(Line::End));
    }

    #[test]
    fn a_huge_line_across_many_buffers_stays_bounded_and_resyncs() {
        let mut data = vec![b'x'; 1_000_000];
        data.extend_from_slice(b"\nnext\n");
        let mut reader = BufReader::with_capacity(16, Cursor::new(data));
        assert_eq!(
            read_bounded_line(&mut reader, 100).ok(),
            Some(Line::TooLong)
        );
        assert_eq!(
            read_bounded_line(&mut reader, 100).ok(),
            Some(Line::Text(b"next".to_vec()))
        );
    }

    #[test]
    fn a_last_line_without_newline_is_returned() {
        let mut input = Cursor::new(b"tail".to_vec());
        assert_eq!(
            read_bounded_line(&mut input, 10).ok(),
            Some(Line::Text(b"tail".to_vec()))
        );
        assert_eq!(read_bounded_line(&mut input, 10).ok(), Some(Line::End));
    }
}
