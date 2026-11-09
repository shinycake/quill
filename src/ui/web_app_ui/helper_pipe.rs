//! The pipes to the web view helper, kept off the UI thread.
//!
//! A hung helper stops reading its stdin, the pipe fills (about 64 KB) and
//! a plain `write_all` then blocks forever. [`HelperWriter`] puts the write
//! on its own thread behind a bounded queue: when the queue is full the
//! helper is treated as wedged and the message is dropped instead of
//! blocking. [`read_bounded_line`] reads the helper's stdout one line at a
//! time without ever holding more than a capped line.

use std::io::Write;
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

pub(super) use quill_webview_protocol::lines::{Line, read_bounded_line};

#[cfg(test)]
mod tests {
    use super::{HelperWriter, Sent};
    use std::io::{self, Write};
    use std::sync::mpsc;
    use std::time::Duration;

    /// A pipe that never drains: it says when a write starts, then parks
    /// that write until released.
    struct StuckPipe {
        entered: mpsc::Sender<()>,
        release: mpsc::Receiver<()>,
    }

    impl Write for StuckPipe {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            let _ = self.entered.send(());
            let _ = self.release.recv();
            Ok(buf.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    /// The writer thread is parked inside the first write, the queue fills,
    /// and every later send is dropped as wedged instead of waiting. No
    /// clock: the test waits only for the writer to take the first message.
    #[test]
    fn a_full_pipe_never_blocks_the_caller() {
        let (entered_tx, entered) = mpsc::channel();
        let (release, release_rx) = mpsc::channel();
        let pipe = StuckPipe {
            entered: entered_tx,
            release: release_rx,
        };
        let writer = HelperWriter::spawn_with_capacity(pipe, 4);
        assert_eq!(writer.send(b"first\n".to_vec()), Sent::Queued);
        // The writer took the first message and is stuck writing it.
        assert!(entered.recv().is_ok());
        let queued: Vec<Sent> = (0..4).map(|_| writer.send(b"x\n".to_vec())).collect();
        assert_eq!(queued, vec![Sent::Queued; 4], "the queue holds four");
        let dropped: Vec<Sent> = (0..16).map(|_| writer.send(b"x\n".to_vec())).collect();
        assert_eq!(dropped, vec![Sent::Wedged; 16], "a full queue drops");
        // Dropping the writer does not wait for the stuck thread.
        drop(writer);
        drop(release);
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
}
