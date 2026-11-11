//! The file being played, read piece by piece from the player.

use std::io::{self, Read, Seek, SeekFrom};

/// How much is asked of the player at once.
const PIECE: u32 = 256 * 1024;

/// A file of `size` bytes, read through `fetch(offset, length)`, which may
/// give fewer bytes than asked only at the end. The last piece is kept, so
/// small reads cost one call per piece.
pub struct Input<F> {
    fetch: F,
    size: u64,
    pos: u64,
    piece_at: u64,
    piece: Vec<u8>,
}

impl<F: FnMut(u64, u32) -> Result<Vec<u8>, String>> Input<F> {
    pub fn new(size: u64, fetch: F) -> Input<F> {
        Input { fetch, size, pos: 0, piece_at: 0, piece: Vec::new() }
    }

    fn in_piece(&self) -> bool {
        self.pos >= self.piece_at && self.pos < self.piece_at + self.piece.len() as u64
    }
}

impl<F: FnMut(u64, u32) -> Result<Vec<u8>, String>> Read for Input<F> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() || self.pos >= self.size {
            return Ok(0);
        }
        if !self.in_piece() {
            self.piece = (self.fetch)(self.pos, PIECE).map_err(io::Error::other)?;
            self.piece_at = self.pos;
            if self.piece.is_empty() {
                return Ok(0);
            }
        }
        let from = (self.pos - self.piece_at) as usize;
        let n = buf.len().min(self.piece.len() - from);
        buf[..n].copy_from_slice(&self.piece[from..from + n]);
        self.pos += n as u64;
        Ok(n)
    }
}

impl<F> Seek for Input<F> {
    fn seek(&mut self, to: SeekFrom) -> io::Result<u64> {
        let pos = match to {
            SeekFrom::Start(n) => Some(n),
            SeekFrom::End(d) => self.size.checked_add_signed(d),
            SeekFrom::Current(d) => self.pos.checked_add_signed(d),
        };
        self.pos = pos.ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "seek before the start"))?;
        Ok(self.pos)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_and_seeks_like_the_bytes_it_fetches_one_piece_at_a_time() {
        let bytes: Vec<u8> = (0..600_000u32).map(|i| (i % 251) as u8).collect();
        let mut calls = 0;
        let mut input = Input::new(bytes.len() as u64, |at, len| {
            calls += 1;
            Ok(bytes[at as usize..(at as usize + len as usize).min(bytes.len())].to_vec())
        });
        let mut head = [0u8; 10];
        input.read_exact(&mut head).unwrap();
        input.read_exact(&mut head).unwrap();
        assert_eq!(head[..], bytes[10..20]);
        assert_eq!(input.seek(SeekFrom::End(-4)).unwrap(), 599_996);
        let mut tail = Vec::new();
        input.read_to_end(&mut tail).unwrap();
        assert_eq!(tail, bytes[599_996..]);
        input.seek(SeekFrom::Start(300_000)).unwrap();
        let mut rest = Vec::new();
        input.read_to_end(&mut rest).unwrap();
        assert_eq!(rest, bytes[300_000..]);
        assert!(input.seek(SeekFrom::Current(-700_000)).is_err());
        drop(input);
        assert_eq!(calls, 4, "one call per piece: the head, the tail, then two from the middle");
    }
}
