//! Progress-bar-aware stderr writer for tracing.

use atlas_core::pipeline;
use std::io::Write;
use tracing_subscriber::fmt::MakeWriter;

/// Logs are written to stderr **with the progress bars parked first**, so a log
/// line never lands in the middle of a bar redraw. stdout stays reserved for a
/// command's actual output.
#[derive(Clone, Copy)]
pub(crate) struct ProgressAwareStderr;

impl<'a> MakeWriter<'a> for ProgressAwareStderr {
    type Writer = ProgressAwareStderr;
    fn make_writer(&'a self) -> Self::Writer {
        *self
    }
}

impl Write for ProgressAwareStderr {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        pipeline::with_suspended_bars(|| {
            let mut err = std::io::stderr().lock();
            err.write_all(buf)?;
            err.flush()?;
            Ok(buf.len())
        })
    }

    fn flush(&mut self) -> std::io::Result<()> {
        pipeline::with_suspended_bars(|| std::io::stderr().flush())
    }
}
