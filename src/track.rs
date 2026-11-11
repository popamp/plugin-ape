//! A Monkey's Audio file as the player plays it: stereo blocks of 32-bit
//! float samples, a frame of the file at a time.

use ape_decoder::ApeDecoder;
use popamp_plugin_api::{Stream, Tags};
use std::io::{Read, Seek};

pub struct Track<R: Read + Seek> {
    decoder: ApeDecoder<R>,
    /// The next frame to decode.
    frame: u32,
    /// Samples (per channel) to drop from the start of that frame, after a seek.
    skip: u32,
}

impl<R: Read + Seek> Track<R> {
    pub fn open(reader: R) -> Result<(Track<R>, Stream), String> {
        let mut decoder = ApeDecoder::new(reader).map_err(|e| format!("not a Monkey's Audio file: {e}"))?;
        let info = decoder.info().clone();
        if !matches!(info.bits_per_sample, 8 | 16 | 24 | 32) || info.channels == 0 || info.sample_rate == 0 {
            return Err(format!("{} channels of {} bits at {} Hz cannot be played", info.channels, info.bits_per_sample, info.sample_rate));
        }
        // A file without a tag, or with a broken one, still plays.
        let tag = decoder.read_tag().ok().flatten();
        let text = |v: Option<&str>| v.map(str::trim).filter(|v| !v.is_empty()).map(String::from);
        let tags = Tags {
            title: text(tag.as_ref().and_then(|t| t.title())),
            artist: text(tag.as_ref().and_then(|t| t.artist())),
            album: text(tag.as_ref().and_then(|t| t.album())),
            year: text(tag.as_ref().and_then(|t| t.year())),
            genre: text(tag.as_ref().and_then(|t| t.genre())),
            comment: text(tag.as_ref().and_then(|t| t.comment())),
            length_ms: Some(info.duration_ms),
            bitrate_kbps: Some(info.average_bitrate_kbps).filter(|&k| k > 0),
            sample_rate: Some(info.sample_rate),
            channels: Some(info.channels),
            codec: Some("Monkey's Audio".into()),
        };
        let stream = Stream { rate: info.sample_rate, length_ms: Some(info.duration_ms), seekable: true, tags };
        Ok((Track { decoder, frame: 0, skip: 0 }, stream))
    }

    /// The next frame, as stereo samples; empty at the end.
    pub fn next_block(&mut self) -> Result<Vec<f32>, String> {
        if self.frame >= self.decoder.total_frames() {
            return Ok(Vec::new());
        }
        let pcm = self.decoder.decode_frame(self.frame).map_err(|e| format!("frame {}: {e}", self.frame))?;
        let info = self.decoder.info();
        let skip = (self.skip as usize * info.block_align as usize).min(pcm.len());
        let block = stereo(&pcm[skip..], info.channels as usize, info.bits_per_sample as usize / 8, info.is_floating_point);
        self.frame += 1;
        self.skip = 0;
        Ok(block)
    }

    /// Goes to `ms`; answers where it landed, to the sample.
    pub fn seek(&mut self, ms: u64) -> Result<u64, String> {
        let rate = self.decoder.info().sample_rate as u64;
        let to = self.decoder.seek(ms * rate / 1000).map_err(|e| e.to_string())?;
        self.frame = to.frame_index;
        self.skip = to.skip_samples;
        Ok(to.actual_sample * 1000 / rate)
    }
}

/// Interleaved PCM as the file holds it, as stereo floats: mono goes to
/// both sides, and of more channels the first two are played.
fn stereo(pcm: &[u8], channels: usize, width: usize, float: bool) -> Vec<f32> {
    let sample = |b: &[u8]| -> f32 {
        match width {
            1 => (b[0] as f32 - 128.0) / 128.0,
            2 => i16::from_le_bytes([b[0], b[1]]) as f32 / 32_768.0,
            3 => (i32::from_le_bytes([0, b[0], b[1], b[2]]) >> 8) as f32 / 8_388_608.0,
            _ if float => f32::from_le_bytes([b[0], b[1], b[2], b[3]]),
            _ => i32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f32 / 2_147_483_648.0,
        }
    };
    pcm.chunks_exact(width * channels)
        .flat_map(|frame| {
            let left = sample(&frame[..width]);
            let right = if channels > 1 { sample(&frame[width..2 * width]) } else { left };
            [left, right]
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;

    fn fixture(name: &str) -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)
    }

    /// The samples of a reference WAV (PCM, little-endian), as stereo floats.
    fn reference(name: &str) -> Vec<f32> {
        let wav = std::fs::read(fixture(name)).unwrap();
        let channels = u16::from_le_bytes([wav[22], wav[23]]);
        let bits = u16::from_le_bytes([wav[34], wav[35]]);
        let mut at = 12;
        while &wav[at..at + 4] != b"data" {
            at += 8 + u32::from_le_bytes(wav[at + 4..at + 8].try_into().unwrap()) as usize;
        }
        let len = u32::from_le_bytes(wav[at + 4..at + 8].try_into().unwrap()) as usize;
        stereo(&wav[at + 8..at + 8 + len], channels as usize, bits as usize / 8, false)
    }

    fn play_all(track: &mut Track<File>) -> Vec<f32> {
        let mut out = Vec::new();
        loop {
            let block = track.next_block().unwrap();
            if block.is_empty() {
                return out;
            }
            out.extend(block);
        }
    }

    #[test]
    fn a_file_plays_as_the_reference_decoder_wrote_it() {
        for (ape, wav, channels) in [
            ("sine_16s_c2000.ape", "sine_16s_c2000.wav", 2),
            ("sine_16m_c2000.ape", "sine_16m_c2000.wav", 1),
            ("sine_24s_c2000.ape", "sine_24s_c2000.wav", 2),
            ("multiframe_16s_c2000.ape", "multiframe_16s_c2000.wav", 2),
        ] {
            let (mut track, stream) = Track::open(File::open(fixture(ape)).unwrap()).unwrap();
            assert_eq!(
                (stream.tags.channels, stream.seekable, stream.tags.codec.as_deref()),
                (Some(channels), true, Some("Monkey's Audio")),
                "{ape}"
            );
            assert_eq!(stream.tags.length_ms, stream.length_ms);
            let out = play_all(&mut track);
            let want = reference(wav);
            assert_eq!(out.len(), want.len(), "{ape}");
            assert!(out == want, "{ape} differs from its reference");
            assert!(out.iter().any(|s| s.abs() > 0.1), "{ape} is audible");
            let ms = out.len() as u64 / 2 * 1000 / stream.rate as u64;
            assert!(ms.abs_diff(stream.length_ms.unwrap()) <= 1, "{ape}: {ms} ms of {:?}", stream.length_ms);
        }
    }

    #[test]
    fn a_seek_lands_on_its_sample_in_any_frame() {
        let (mut track, stream) = Track::open(File::open(fixture("multiframe_16s_c2000.ape")).unwrap()).unwrap();
        let all = play_all(&mut track);
        let length = stream.length_ms.unwrap();
        for ms in [0, 1, length / 3, length * 2 / 3, length - 10] {
            let reached = track.seek(ms).unwrap();
            assert!(reached <= ms && ms - reached <= 1, "{ms} ms reached {reached}: the sample's time, rounded down");
            let rest = play_all(&mut track);
            let from = (ms * stream.rate as u64 / 1000) as usize * 2;
            assert!(rest == all[from..], "from {ms} ms");
        }
    }

    #[test]
    fn a_tagged_file_names_itself() {
        let mut file = std::io::Cursor::new(std::fs::read(fixture("sine_16s_c2000.ape")).unwrap());
        let mut tag = ape_decoder::ApeTag::new();
        for (key, value) in [("Title", "Sine"), ("Artist", "Popamp"), ("Album", "Tests"), ("Year", "2026"), ("Genre", " ")] {
            tag.set(key, value);
        }
        ape_decoder::write_tag(&mut file, &tag).unwrap();
        file.set_position(0);
        let (mut track, stream) = Track::open(file).unwrap();
        let t = &stream.tags;
        assert_eq!((t.title.as_deref(), t.artist.as_deref(), t.album.as_deref()), (Some("Sine"), Some("Popamp"), Some("Tests")));
        assert_eq!((t.year.as_deref(), t.genre.as_deref()), (Some("2026"), None), "a blank field is none");
        assert!(!track.next_block().unwrap().is_empty(), "the tag at the end does not stop it playing");
    }

    #[test]
    fn a_file_that_is_not_monkeys_audio_says_so() {
        let err = Track::open(File::open(fixture("sine_16s_c2000.wav")).unwrap()).err().unwrap();
        assert!(err.starts_with("not a Monkey's Audio file"), "{err}");
    }
}
