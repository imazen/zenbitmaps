//! Incremental (sans-I/O) decoding shared by the raw bitmap formats.
//!
//! A [`StripFormat`] says where a file's rows are and how to turn a run of
//! them into pixels with the same functions the one-shot decoder uses;
//! [`StripDecoder`] runs zencodec's `step` protocol over it. Three body
//! shapes cover the formats:
//!
//! - [`Body::Rows`]: fixed-size rows (PNM binary, PFM, farbfeld). Rows are
//!   decoded straight from the driver's bytes, a strip at a time. Top-down
//!   files are read forward with `AtLeast` one row; bottom-up files (PFM) are
//!   read as exact `Range`s from the end, so a random-access source never
//!   holds more than one strip.
//! - [`Body::VarRows`]: variable-length rows with a bounded worst case (HDR
//!   scanlines). The decoder asks for one worst-case row and decodes every
//!   row the offered bytes guarantee.
//! - [`Body::Whole`]: formats whose rows cannot be located or decoded without
//!   the rest of the file (ASCII PNM, RLE, BMP). The header is reported as
//!   soon as it arrives; the pixels come from the format's one-shot decode
//!   once the whole file is offered, so they are identical to it.
//!
//! Every path ends in the one-shot code: rows are decoded by the same
//! per-image functions applied to a strip, and converted by
//! [`super::layout_to_pixel_buffer`], so the incremental pixels equal the
//! one-shot pixels byte for byte (tests/sansio_bitmaps.rs checks every
//! format at every chunk size).

use enough::Stop;
use whereat::At;
use zencodec::decode::{
    BatchMeta, DecodeRowSink, Event, IncrementalDecode, Input, InputRequest, MetadataUpdate,
    OutputInfo, Progress, StepError,
};
use zencodec::{CodecError, ImageInfo};
use zenpixels::{PixelBuffer, PixelDescriptor, PixelSlice};

use crate::PixelLayout;
use crate::error::BitmapError;

/// Output bytes per strip handed to the sink, at most (a single row is
/// always allowed).
const STRIP_TARGET_BYTES: usize = 256 * 1024;

/// Where a file's pixel rows are.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Body {
    /// Fixed-size rows starting at `data_offset`.
    Rows {
        data_offset: u64,
        row_bytes: usize,
        /// The file stores the last row first.
        bottom_up: bool,
    },
    /// Variable-length rows starting at `data_offset`, none longer than
    /// `worst_row_bytes`.
    #[cfg_attr(not(feature = "hdr"), allow(dead_code))]
    VarRows {
        data_offset: u64,
        worst_row_bytes: usize,
    },
    /// Decoded by the one-shot path from the complete file.
    Whole,
}

/// A parsed header.
pub(crate) struct Geometry {
    pub width: u32,
    pub height: u32,
    /// Internal layout of decoded rows (unused for [`Body::Whole`]).
    pub layout: PixelLayout,
    pub info: ImageInfo,
    pub body: Body,
}

/// One raw bitmap format, as the strip decoder sees it.
pub(crate) trait StripFormat: Send + 'static {
    /// Parse the header from the first bytes of the file, running the same
    /// validation and limit checks the one-shot decoder runs before it
    /// reads pixels. `Ok(None)` means `prefix` is too short to decide and
    /// more bytes may follow (`at_end` is false).
    fn geometry(&mut self, prefix: &[u8], at_end: bool) -> crate::Result<Option<Geometry>>;

    /// [`Body::Rows`]: decode the `n` rows whose file bytes (in file order)
    /// are exactly `src`, returning them in display order.
    /// [`Body::VarRows`]: decode `n` rows from the start of `src` (which holds
    /// at least `n` worst-case rows, or every remaining byte), returning the
    /// rows and the bytes they used.
    fn decode_rows<'s>(
        &mut self,
        src: &'s [u8],
        n: u32,
        stop: &dyn Stop,
    ) -> crate::Result<(crate::decode::DecodeOutput<'s>, usize)>;

    /// [`Body::Whole`]: decode the complete file through the one-shot path.
    fn decode_whole(
        &mut self,
        file: &[u8],
        stop: &dyn Stop,
    ) -> Result<zencodec::decode::DecodeOutput, At<CodecError>> {
        let _ = (file, stop);
        Err(BitmapError::InvalidData("format has no whole-file decode".into()).into())
    }
}

type StepResult = Result<Progress, StepError<At<CodecError>>>;

/// A pulse that stops inside native decoding has tripped exactly as at the
/// step boundary: report `Cancelled` with its reason, not a codec failure.
fn codec(e: impl Into<At<CodecError>>) -> StepError<At<CodecError>> {
    let e = e.into();
    match e.error().category() {
        zencodec::ErrorCategory::Stopped(reason) => StepError::Cancelled(reason),
        _ => StepError::Codec(e),
    }
}

fn bitmap(e: At<BitmapError>) -> StepError<At<CodecError>> {
    match e.error() {
        BitmapError::Cancelled(reason) => StepError::Cancelled(*reason),
        _ => StepError::Codec(CodecError::of(e)),
    }
}

/// The incremental decoder for a [`StripFormat`].
pub(crate) struct StripDecoder<F> {
    format: F,
    geometry: Option<Geometry>,
    /// Descriptor rows are emitted in, once known.
    descriptor: Option<PixelDescriptor>,
    /// Internal rows already carry the emitted bytes (no conversion).
    passthrough: bool,
    began: bool,
    next_row: u32,
    /// Absolute offset of the next byte (sequential bodies).
    pos: u64,
    rows_per_strip: u32,
    /// A whole-file decode waiting to be handed to the sink.
    whole: Option<PixelBuffer>,
    header_want: usize,
    need: [InputRequest; 1],
    done: Option<OutputInfo>,
    failed: bool,
    pulse: zencodec::SharedPulse,
    max_input_bytes: Option<u64>,
}

impl<F: StripFormat> StripDecoder<F> {
    pub(crate) fn new(
        format: F,
        pulse: zencodec::SharedPulse,
        max_input_bytes: Option<u64>,
    ) -> Self {
        Self {
            format,
            geometry: None,
            descriptor: None,
            passthrough: false,
            began: false,
            next_row: 0,
            pos: 0,
            rows_per_strip: 1,
            whole: None,
            header_want: 512,
            need: [InputRequest::at_least(0, 512)],
            done: None,
            failed: false,
            pulse,
            max_input_bytes,
        }
    }

    fn need(&mut self, request: InputRequest) -> StepResult {
        self.need = [request];
        Ok(Progress::need(0))
    }

    fn check_input_limit(&self, end: Option<u64>) -> Result<(), StepError<At<CodecError>>> {
        if let (Some(max), Some(end)) = (self.max_input_bytes, end)
            && end > max
        {
            return Err(codec(BitmapError::LimitExceeded(alloc::format!(
                "input size {end} exceeds limit {max}"
            ))));
        }
        Ok(())
    }

    /// Hand `rows` (internal layout, display order) to the sink at `y`.
    fn emit_internal(
        &mut self,
        sink: &mut dyn DecodeRowSink,
        y: u32,
        rows: &crate::decode::DecodeOutput<'_>,
    ) -> Result<(), StepError<At<CodecError>>> {
        let desc = self.descriptor.expect("set with geometry");
        if self.passthrough {
            let row_bytes = rows.width as usize * rows.layout.bytes_per_pixel();
            let mut dst = sink
                .provide_next_buffer(y, rows.height, rows.width, desc)
                .map_err(StepError::Sink)?;
            for (r, src) in rows.pixels().chunks_exact(row_bytes).enumerate() {
                dst.row_mut(r as u32)[..row_bytes].copy_from_slice(src);
            }
            return Ok(());
        }
        let buf = super::layout_to_pixel_buffer(rows).map_err(bitmap)?;
        copy_rows(sink, y, buf.as_slice())
    }

    fn begin(
        &mut self,
        sink: &mut dyn DecodeRowSink,
        width: u32,
        height: u32,
        desc: PixelDescriptor,
    ) -> Result<(), StepError<At<CodecError>>> {
        self.descriptor = Some(desc);
        self.rows_per_strip = rows_per_strip(width, height, desc);
        sink.begin(width, height, desc).map_err(StepError::Sink)?;
        self.began = true;
        Ok(())
    }

    fn finish(&mut self, sink: &mut dyn DecodeRowSink) -> StepResult {
        sink.finish().map_err(StepError::Sink)?;
        let g = self.geometry.as_ref().expect("header parsed");
        let info = OutputInfo::full_decode(g.width, g.height, self.descriptor.expect("began"));
        self.done = Some(info.clone());
        Ok(Progress::new(0, Event::Done(info)))
    }

    fn step_header(&mut self, input: Input<'_>, sink: &mut dyn DecodeRowSink) -> StepResult {
        let here: &[u8] = if input.offset == 0 { input.bytes } else { &[] };
        let at_end = input.offset == 0 && input.reaches_end() || input.total_len == Some(0);
        let Some(g) = self.format.geometry(here, at_end).map_err(bitmap)? else {
            if at_end {
                return Err(codec(BitmapError::UnexpectedEof));
            }
            let want = self.header_want.max(here.len() + 1);
            self.header_want = want.saturating_mul(2);
            return self.need(InputRequest::at_least(0, want));
        };
        let consumed = match g.body {
            Body::Rows { data_offset, .. } | Body::VarRows { data_offset, .. } => {
                let layout_desc = emitted_descriptor(g.layout).map_err(bitmap)?;
                self.passthrough = matches!(
                    g.layout,
                    PixelLayout::Gray8
                        | PixelLayout::Rgb8
                        | PixelLayout::Rgba8
                        | PixelLayout::Bgra8
                );
                self.begin(sink, g.width, g.height, layout_desc)?;
                self.pos = data_offset;
                data_offset as usize
            }
            // The whole file is decoded later, from offset 0.
            Body::Whole => 0,
        };
        self.geometry = Some(g);
        Ok(Progress::new(
            consumed,
            Event::Metadata(MetadataUpdate::HeaderReady),
        ))
    }

    fn step_inner(
        &mut self,
        input: Input<'_>,
        sink: &mut dyn DecodeRowSink,
        pulse: &dyn zencodec::Pulse,
    ) -> StepResult {
        pulse.check()?;
        self.pulse.check()?;
        self.check_input_limit(input.total_len)?;
        let Some(g) = self.geometry.as_ref() else {
            return self.step_header(input, sink);
        };
        let (width, height, body) = (g.width, g.height, g.body);
        let remaining = height - self.next_row;

        if let Some(buf) = self.whole.take() {
            let y = self.next_row;
            let n = remaining.min(self.rows_per_strip);
            let all = buf.as_slice();
            let copied = copy_rows(sink, y, all.sub_rows(y, n));
            self.whole = Some(buf);
            copied?;
            self.next_row += n;
            if self.next_row == height {
                self.whole = None;
            }
            return Ok(Progress::new(
                0,
                Event::Rows {
                    meta: BatchMeta::final_rows(y),
                    rows: n,
                },
            ));
        }
        // A whole-file body whose header never probed has no height yet:
        // decode it before deciding it is done.
        if remaining == 0 && self.began {
            return self.finish(sink);
        }

        match body {
            Body::Rows {
                data_offset,
                row_bytes,
                bottom_up: false,
            } => {
                let off = data_offset + self.next_row as u64 * row_bytes as u64;
                let here: &[u8] = if input.offset == off {
                    input.bytes
                } else {
                    &[]
                };
                let at_end = input.offset == off && input.reaches_end()
                    || input.total_len.is_some_and(|t| t <= off);
                let n = (here.len() / row_bytes).min(remaining as usize) as u32;
                let n = n.min(self.rows_per_strip);
                if n == 0 {
                    if at_end {
                        return Err(codec(BitmapError::UnexpectedEof));
                    }
                    return self.need(InputRequest::at_least(off, row_bytes));
                }
                let used = n as usize * row_bytes;
                self.check_input_limit(Some(off + used as u64))?;
                let (rows, _) = self
                    .format
                    .decode_rows(&here[..used], n, pulse)
                    .map_err(bitmap)?;
                let y = self.next_row;
                self.emit_internal(sink, y, &rows)?;
                self.next_row += n;
                Ok(Progress::new(
                    used,
                    Event::Rows {
                        meta: BatchMeta::final_rows(y),
                        rows: n,
                    },
                ))
            }
            Body::Rows {
                data_offset,
                row_bytes,
                bottom_up: true,
            } => {
                // Display rows y..y+n are file rows height-y-n..height-y.
                let n = remaining.min(self.rows_per_strip);
                let first_file_row = height - self.next_row - n;
                let off = data_offset + first_file_row as u64 * row_bytes as u64;
                let len = n as usize * row_bytes;
                if input.total_len.is_some_and(|t| t < off + len as u64) {
                    return Err(codec(BitmapError::UnexpectedEof));
                }
                if input.offset != off || input.bytes.len() < len {
                    return self.need(InputRequest::range(off, len as u64));
                }
                let (rows, _) = self
                    .format
                    .decode_rows(&input.bytes[..len], n, pulse)
                    .map_err(bitmap)?;
                let y = self.next_row;
                self.emit_internal(sink, y, &rows)?;
                self.next_row += n;
                Ok(Progress::new(
                    len,
                    Event::Rows {
                        meta: BatchMeta::final_rows(y),
                        rows: n,
                    },
                ))
            }
            Body::VarRows {
                worst_row_bytes, ..
            } => {
                let off = self.pos;
                let here: &[u8] = if input.offset == off {
                    input.bytes
                } else {
                    &[]
                };
                let at_end = input.offset == off && input.reaches_end()
                    || input.total_len.is_some_and(|t| t <= off);
                let guaranteed = if at_end {
                    remaining
                } else {
                    (here.len() / worst_row_bytes).min(remaining as usize) as u32
                };
                let n = guaranteed.min(self.rows_per_strip);
                if n == 0 {
                    return self.need(InputRequest::at_least(off, worst_row_bytes));
                }
                let (rows, used) = self.format.decode_rows(here, n, pulse).map_err(bitmap)?;
                self.check_input_limit(Some(off + used as u64))?;
                let y = self.next_row;
                self.emit_internal(sink, y, &rows)?;
                self.next_row += n;
                self.pos += used as u64;
                Ok(Progress::new(
                    used,
                    Event::Rows {
                        meta: BatchMeta::final_rows(y),
                        rows: n,
                    },
                ))
            }
            Body::Whole => {
                if !(input.offset == 0 && input.reaches_end()) {
                    // Ask for everything: the total when known, otherwise a
                    // prefix that keeps doubling, which a stream serves by
                    // reading on until it ends.
                    let want = match input.total_len {
                        Some(total) => total as usize,
                        None => {
                            let held = if input.offset == 0 {
                                input.bytes.len()
                            } else {
                                0
                            };
                            held.saturating_mul(2).max(held + 64 * 1024)
                        }
                    };
                    return self.need(InputRequest::at_least(0, want));
                }
                let out = self
                    .format
                    .decode_whole(input.bytes, pulse)
                    .map_err(codec)?;
                let buf = out.into_buffer();
                let s = buf.as_slice();
                let (w, h, desc) = (s.width(), s.rows(), s.descriptor());
                if width != 0 && (w, h) != (width, height) {
                    return Err(codec(BitmapError::InvalidData(alloc::format!(
                        "header said {width}x{height}, decode produced {w}x{h}"
                    ))));
                }
                if let Some(g) = self.geometry.as_mut() {
                    (g.width, g.height) = (w, h);
                }
                self.begin(sink, w, h, desc)?;
                self.whole = Some(buf);
                Ok(Progress::new(
                    input.bytes.len(),
                    Event::Metadata(MetadataUpdate::Updated),
                ))
            }
        }
    }
}

impl<F: StripFormat> IncrementalDecode for StripDecoder<F> {
    type Error = At<CodecError>;

    fn step(
        &mut self,
        input: Input<'_>,
        sink: &mut dyn DecodeRowSink,
        pulse: Option<&dyn zencodec::Pulse>,
    ) -> StepResult {
        if let Some(done) = &self.done {
            return Ok(Progress::new(0, Event::Done(done.clone())));
        }
        if self.failed {
            return Err(StepError::Finished);
        }
        let r = self.step_inner(input, sink, pulse.unwrap_or(&zencodec::NoPulse));
        self.failed = r.is_err();
        r
    }

    fn requested(&self) -> &[InputRequest] {
        &self.need
    }

    fn info(&self) -> Option<&ImageInfo> {
        self.geometry.as_ref().map(|g| &g.info)
    }

    fn output_descriptor(&self) -> Option<PixelDescriptor> {
        self.descriptor
    }

    fn access_pattern(&self) -> zencodec::source::AccessPattern {
        match self.geometry.as_ref().map(|g| g.body) {
            Some(Body::Rows {
                bottom_up: true, ..
            }) => zencodec::source::AccessPattern::RandomAccess,
            _ => zencodec::source::AccessPattern::Sequential,
        }
    }
}

/// Strip height: as many rows as fit [`STRIP_TARGET_BYTES`] of output.
fn rows_per_strip(width: u32, height: u32, desc: PixelDescriptor) -> u32 {
    let row = (width as usize)
        .saturating_mul(desc.bytes_per_pixel())
        .max(1);
    ((STRIP_TARGET_BYTES / row).max(1) as u32).min(height.max(1))
}

/// The descriptor the one-shot path labels a layout with, taken from the
/// one-shot conversion itself so the two can never disagree.
pub(crate) fn emitted_descriptor(layout: PixelLayout) -> crate::Result<PixelDescriptor> {
    let one = alloc::vec![0u8; layout.bytes_per_pixel()];
    let probe = crate::decode::DecodeOutput::owned(one, 1, 1, layout);
    Ok(super::layout_to_pixel_buffer(&probe)?
        .as_slice()
        .descriptor())
}

fn copy_rows(
    sink: &mut dyn DecodeRowSink,
    y: u32,
    src: PixelSlice<'_>,
) -> Result<(), StepError<At<CodecError>>> {
    let (w, n, desc) = (src.width(), src.rows(), src.descriptor());
    let row_bytes = w as usize * desc.bytes_per_pixel();
    let mut dst = sink
        .provide_next_buffer(y, n, w, desc)
        .map_err(StepError::Sink)?;
    for r in 0..n {
        dst.row_mut(r)[..row_bytes].copy_from_slice(&src.row(r)[..row_bytes]);
    }
    Ok(())
}

#[cfg(any(feature = "bmp", feature = "tga"))]
/// A format decoded only from the complete file: the header is reported as
/// soon as the job's own `probe` accepts the prefix, and the pixels come from
/// the job's own one-shot decoder, so they are exactly the one-shot pixels.
pub(crate) struct WholeFile<J> {
    job: Option<J>,
    format: zencodec::ImageFormat,
}

#[cfg(any(feature = "bmp", feature = "tga"))]
impl<J> WholeFile<J> {
    pub(crate) fn new(job: J, format: zencodec::ImageFormat) -> Self {
        Self {
            job: Some(job),
            format,
        }
    }
}

#[cfg(any(feature = "bmp", feature = "tga"))]
impl<J> StripFormat for WholeFile<J>
where
    J: for<'a> zencodec::decode::DecodeJob<'a, Error = At<CodecError>> + Send + 'static,
{
    fn geometry(&mut self, prefix: &[u8], at_end: bool) -> crate::Result<Option<Geometry>> {
        let job = self.job.as_ref().expect("geometry before decode");
        // A prefix may fail to probe for want of bytes; the decoder reports
        // the real error once the whole file is there.
        let info = match job.probe(prefix) {
            Ok(info) => info,
            Err(_) if !at_end => return Ok(None),
            // The whole file is here and still does not probe: let the
            // one-shot decoder report its own error (dimensions stay 0, which
            // the strip decoder reads as "unknown").
            Err(_) => ImageInfo::new(0, 0, self.format),
        };
        Ok(Some(Geometry {
            width: info.width,
            height: info.height,
            layout: PixelLayout::Rgba8,
            info,
            body: Body::Whole,
        }))
    }

    fn decode_rows<'s>(
        &mut self,
        _src: &'s [u8],
        _n: u32,
        _stop: &dyn Stop,
    ) -> crate::Result<(crate::decode::DecodeOutput<'s>, usize)> {
        Err(whereat::at!(BitmapError::InvalidData(
            "whole-file format has no row decode".into()
        )))
    }

    fn decode_whole(
        &mut self,
        file: &[u8],
        _stop: &dyn Stop,
    ) -> Result<zencodec::decode::DecodeOutput, At<CodecError>> {
        use zencodec::decode::Decode as _;
        let job = self.job.take().ok_or_else(|| {
            At::<CodecError>::from(BitmapError::InvalidData("decoded twice".into()))
        })?;
        job.decoder(alloc::borrow::Cow::Borrowed(file), &[])?
            .decode()
    }
}
