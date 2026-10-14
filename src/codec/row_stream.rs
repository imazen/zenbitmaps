//! Streaming output for formats whose file is a header naming the height
//! followed by independently encoded rows (PNM P5/P6/P7, farbfeld, HDR).
//!
//! After `Encoder::begin` the header is final, so each pushed strip's rows
//! are encoded at once and handed out through `pending_output`. Without
//! `begin`, the encoded rows wait and the header is written at `finish`. In
//! both cases the bytes are those of the format's one-shot encoder run on the
//! same pixels: the header comes from the same writer, and each strip is the
//! one-shot encoder's output for those rows minus its own header.

use alloc::vec::Vec;

use enough::Stop;
use whereat::At;
use zencodec::CodecError;
use zenpixels::{ChannelLayout, ChannelType, PixelDescriptor, PixelSlice};

use crate::PixelLayout;
use crate::error::BitmapError;
use crate::limits::Limits;

/// Which file is being written.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum RowFile {
    Pnm(crate::pnm::PnmFormat, PixelLayout),
    Farbfeld(PixelLayout),
    #[cfg(feature = "hdr")]
    Hdr(PixelLayout),
}

impl RowFile {
    fn header(self, width: u32, height: u32, stop: &dyn Stop) -> crate::Result<Vec<u8>> {
        match self {
            Self::Pnm(fmt, layout) => {
                crate::pnm::encode::encode_parts(&[], width, height, 0, layout, fmt, stop)
            }
            Self::Farbfeld(_) => {
                let mut v = Vec::with_capacity(16);
                v.extend_from_slice(b"farbfeld");
                v.extend_from_slice(&width.to_be_bytes());
                v.extend_from_slice(&height.to_be_bytes());
                Ok(v)
            }
            #[cfg(feature = "hdr")]
            Self::Hdr(_) => {
                let mut v = Vec::new();
                crate::hdr::encode::write_header(&mut v, width, height);
                Ok(v)
            }
        }
    }

    /// The encoded rows of `pixels` (`rows` rows), without a header.
    fn rows(self, pixels: &[u8], width: u32, rows: u32, stop: &dyn Stop) -> crate::Result<Vec<u8>> {
        let (mut file, header_len) = match self {
            Self::Pnm(fmt, layout) => {
                let file =
                    crate::pnm::encode::encode_parts(pixels, width, rows, rows, layout, fmt, stop)?;
                let header = self.header(width, rows, stop)?.len();
                (file, header)
            }
            Self::Farbfeld(layout) => (
                crate::farbfeld::encode(pixels, width, rows, layout, stop)?,
                16,
            ),
            #[cfg(feature = "hdr")]
            Self::Hdr(layout) => {
                let file = crate::hdr::encode(pixels, width, rows, layout, stop)?;
                let header = self.header(width, rows, stop)?.len();
                (file, header)
            }
        };
        file.drain(..header_len);
        Ok(file)
    }
}

/// The streaming state of one encode.
pub(crate) struct RowStream {
    file: RowFile,
    descriptor: PixelDescriptor,
    width: u32,
    rows: u32,
    declared: Option<u32>,
    /// With `begin`: the file so far, of which `out[..drained]` was handed
    /// out. Without: the encoded rows, the header still to come.
    out: Vec<u8>,
    drained: usize,
    limits: Option<Limits>,
}

fn err(e: BitmapError) -> At<CodecError> {
    At::<CodecError>::from(e)
}

impl RowStream {
    /// Start a stream for rows in `descriptor`, choosing the file with
    /// `pick`. `height` is the declared height (`begin`), if any.
    pub(crate) fn start(
        file: RowFile,
        width: u32,
        height: Option<u32>,
        descriptor: PixelDescriptor,
        limits: Option<Limits>,
        stop: &dyn Stop,
    ) -> Result<Self, At<CodecError>> {
        if let (Some(l), Some(h)) = (&limits, height) {
            l.check(width, h).map_err(CodecError::of)?;
        }
        let out = match height {
            Some(h) => file.header(width, h, stop).map_err(CodecError::of)?,
            None => Vec::new(),
        };
        Ok(Self {
            file,
            descriptor,
            width,
            rows: 0,
            declared: height,
            out,
            drained: 0,
            limits,
        })
    }

    /// Encode and append a strip.
    pub(crate) fn push(
        &mut self,
        rows: PixelSlice<'_>,
        stop: &dyn Stop,
    ) -> Result<(), At<CodecError>> {
        if rows.width() != self.width || rows.descriptor() != self.descriptor {
            return Err(err(BitmapError::InvalidData(
                "push_rows: width or pixel format changed".into(),
            )));
        }
        let total = self.rows.checked_add(rows.rows()).ok_or_else(|| {
            err(BitmapError::DimensionsTooLarge {
                width: self.width,
                height: u32::MAX,
            })
        })?;
        if let Some(declared) = self.declared
            && total > declared
        {
            return Err(err(BitmapError::InvalidData(alloc::format!(
                "push_rows: more than the {declared} rows declared at begin"
            ))));
        }
        if let Some(l) = &self.limits {
            l.check(self.width, total).map_err(CodecError::of)?;
        }
        let bytes = rows.contiguous_bytes();
        let encoded = self
            .file
            .rows(&bytes, self.width, rows.rows(), stop)
            .map_err(CodecError::of)?;
        self.out.extend_from_slice(&encoded);
        self.rows = total;
        Ok(())
    }

    pub(crate) fn pending(&self) -> &[u8] {
        match self.declared {
            Some(_) => &self.out[self.drained..],
            None => &[],
        }
    }

    pub(crate) fn consume(&mut self, n: usize) {
        if self.declared.is_none() {
            return;
        }
        self.drained = (self.drained + n).min(self.out.len());
        if self.drained == self.out.len() {
            self.out.clear();
            self.drained = 0;
        } else if self.drained >= 64 * 1024 && self.drained > self.out.len() / 2 {
            self.out.drain(..self.drained);
            self.drained = 0;
        }
    }

    /// The bytes not yet handed out: the rest of the file.
    pub(crate) fn finish(mut self, stop: &dyn Stop) -> Result<Vec<u8>, At<CodecError>> {
        match self.declared {
            Some(declared) if declared != self.rows => {
                Err(err(BitmapError::InvalidData(alloc::format!(
                    "finish: {} rows pushed, {declared} declared at begin",
                    self.rows
                ))))
            }
            Some(_) => {
                self.out.drain(..self.drained);
                Ok(self.out)
            }
            None => {
                let mut file = self
                    .file
                    .header(self.width, self.rows, stop)
                    .map_err(CodecError::of)?;
                file.extend_from_slice(&self.out);
                Ok(file)
            }
        }
    }
}

/// The PNM file the one-shot encoder writes for `desc`, minus the
/// load-bearing narrowing a streaming encoder cannot know about in advance.
/// `None` for PFM (stored bottom-up, so it cannot stream) and unsupported
/// descriptors.
pub(crate) fn pnm_file(desc: PixelDescriptor) -> Option<RowFile> {
    use crate::pnm::PnmFormat;
    Some(match (desc.channel_type(), desc.layout()) {
        (ChannelType::U8, ChannelLayout::Rgb) => RowFile::Pnm(PnmFormat::Ppm, PixelLayout::Rgb8),
        (ChannelType::U8, ChannelLayout::Rgba) => RowFile::Pnm(PnmFormat::Pam, PixelLayout::Rgba8),
        (ChannelType::U8, ChannelLayout::Gray) => RowFile::Pnm(PnmFormat::Pgm, PixelLayout::Gray8),
        (ChannelType::U8, ChannelLayout::Bgra) => RowFile::Pnm(PnmFormat::Ppm, PixelLayout::Bgra8),
        _ => return None,
    })
}
