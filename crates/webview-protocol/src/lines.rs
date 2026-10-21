//! Reading the other side's lines without trusting their length.

use std::io::{self, BufRead};

/// One line read by [`read_bounded_line`].
#[derive(Debug, PartialEq, Eq)]
pub enum Line {
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
pub fn read_bounded_line(reader: &mut impl BufRead, cap: usize) -> io::Result<Line> {
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
    use super::{Line, read_bounded_line};
    use std::io::{BufReader, Cursor};

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
