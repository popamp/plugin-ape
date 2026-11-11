//! Monkey's Audio for Popamp: a decoder plug-in for `.ape` files, through
//! the pure-Rust [ape-decoder](https://crates.io/crates/ape-decoder). The
//! calls are described in
//! [popamp-plugin-api](https://github.com/popamp/popamp-plugin-api).

pub mod input;
pub mod track;

#[cfg(target_arch = "wasm32")]
mod plugin {
    use crate::input::Input;
    use crate::track::Track;
    use extism_pdk::*;
    use popamp_plugin_api::{FileInfo, Range, Stream, Version, VERSION};
    use std::cell::RefCell;

    #[host_fn]
    extern "ExtismHost" {
        fn popamp_input(range: Json<Range>) -> Vec<u8>;
    }

    fn fetch(offset: u64, length: u32) -> Result<Vec<u8>, String> {
        unsafe { popamp_input(Json(Range { offset, length })) }.map_err(|e| e.to_string())
    }

    type File = Input<fn(u64, u32) -> Result<Vec<u8>, String>>;

    thread_local! {
        /// The file this instance plays; the player gives each its own.
        static TRACK: RefCell<Option<Track<File>>> = const { RefCell::new(None) };
    }

    fn failed(e: String) -> WithReturnCode<Error> {
        WithReturnCode::new(Error::msg(e), 1)
    }

    fn with_track<T>(f: impl FnOnce(&mut Track<File>) -> Result<T, String>) -> FnResult<T> {
        TRACK.with_borrow_mut(|t| f(t.as_mut().ok_or("no file is open")?)).map_err(failed)
    }

    #[plugin_fn]
    pub fn api_version(_: String) -> FnResult<Json<Version>> {
        Ok(Json(VERSION))
    }

    #[plugin_fn]
    pub fn open_file(Json(file): Json<FileInfo>) -> FnResult<Json<Stream>> {
        let (track, stream) = Track::open(Input::new(file.size, fetch as fn(u64, u32) -> Result<Vec<u8>, String>)).map_err(failed)?;
        TRACK.set(Some(track));
        Ok(Json(stream))
    }

    #[plugin_fn]
    pub fn decode(_: Vec<u8>) -> FnResult<Vec<u8>> {
        with_track(|t| t.next_block()).map(|block| popamp_plugin_api::bytes_of(&block))
    }

    #[plugin_fn]
    pub fn seek(Json(ms): Json<u64>) -> FnResult<Json<u64>> {
        with_track(|t| t.seek(ms)).map(Json)
    }
}
