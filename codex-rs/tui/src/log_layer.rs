//! Custom `tracing_subscriber` layer that forwards every formatted log event to the
//! TUI so the status indicator can display the *latest* log line while a task is
//! running.
//!
//! The layer is intentionally extremely small: we implement `on_event()` only and
//! ignore spans/metadata because we only care about the already‑formatted output
//! that the default `fmt` layer would print.  We therefore borrow the same
//! formatter (`tracing_subscriber::fmt::format::FmtSpan`) used by the default
//! fmt layer so the text matches what is written to the log file.

use std::fmt::Write as _;

use tokio::sync::mpsc::UnboundedSender;
use tracing::Event;
use tracing::Subscriber;
use tracing::field::Field;
use tracing::field::Visit;
use tracing_subscriber::Layer;
use tracing_subscriber::layer::Context;
use tracing_subscriber::registry::LookupSpan;

pub struct TuiLogLayer {
    tx: UnboundedSender<String>,
}

impl TuiLogLayer {
    pub fn new(tx: UnboundedSender<String>) -> Self {
        Self {
            tx,
        }
    }
}

impl<S> Layer<S> for TuiLogLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        // Build a terse line like `[TRACE core::session] message …` by visiting
        // fields into a buffer. This avoids pulling in the heavyweight
        // formatter machinery.

        struct Visitor<'a> {
            buf: &'a mut String,
        }

        impl Visit for Visitor<'_> {
            fn record_debug(&mut self, _field: &Field, value: &dyn std::fmt::Debug) {
                let _ = write!(self.buf, " {:?}", value);
            }
        }

        let mut buf = String::new();
        let _ = write!(
            buf,
            "[{} {}]",
            event.metadata().level(),
            event.metadata().target()
        );

        event.record(&mut Visitor { buf: &mut buf });

        let sanitized = buf.replace(['\n', '\r'], " ");
        let _ = self.tx.send(sanitized);
    }
}
