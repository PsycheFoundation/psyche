//! A [`tracing_subscriber::Layer`] that `error` and `warning` events to the
//! global [`EventStore`] as [`client::Error`] and [`client::Warning`] events.
//!
//! # Usage
//!
//! Add this layer when building your subscriber stack, typically inside the
//! psyche-logging crate:
//!
//! ```ignore
//! use tracing_subscriber::prelude::*;
//! use psyche_event_sourcing::EventStoreTracingLayer;
//!
//! let subscriber = tracing_subscriber::registry()
//!     .with(EventStoreTracingLayer)
//!     .with(/* your other layers */);
//!
//! tracing::subscriber::set_global_default(subscriber).unwrap();
//! ```

use tracing::{Event, Level, Subscriber};
use tracing_subscriber::Layer;
use tracing_subscriber::layer::Context;

use crate::EventStore;
use crate::events::client;

/// Forwards `error!()` tracing events to the global [`EventStore`].
///
/// Only `ERROR`-level events are captured; all other levels are ignored.
/// The message is formatted as `[target] <message> (field=value, …)`.
pub struct EventStoreTracingLayer;

impl<S: Subscriber> Layer<S> for EventStoreTracingLayer {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let level = *event.metadata().level();
        if level > Level::WARN {
            return;
        }

        let mut collector = FieldCollector::default();
        event.record(&mut collector);

        let target = event.metadata().target();
        let message = if collector.extra.is_empty() {
            format!("[{target}] {}", collector.message)
        } else {
            format!(
                "[{target}] {} ({})",
                collector.message,
                collector.extra.join(", ")
            )
        };

        EventStore::emit(
            if level == Level::ERROR {
                client::Error { message }.into()
            } else {
                client::Warning { message }.into()
            },
            chrono::Utc::now(),
        );
    }
}

/// Collects tracing event fields into a message string and extra key=value pairs.
#[derive(Default)]
struct FieldCollector {
    message: String,
    extra: Vec<String>,
}

impl tracing::field::Visit for FieldCollector {
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "message" {
            self.message = value.to_string();
        } else {
            self.extra.push(format!("{}={}", field.name(), value));
        }
    }

    fn record_error(
        &mut self,
        field: &tracing::field::Field,
        value: &(dyn std::error::Error + 'static),
    ) {
        if field.name() == "message" {
            self.message = value.to_string();
        } else {
            self.extra.push(format!("{}={}", field.name(), value));
        }
    }

    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            // tracing formats the message field with Display, not Debug, but it
            // comes through record_debug — strip the outer quotes if present.
            let s = format!("{value:?}");
            self.message = s.trim_matches('"').to_string();
        } else {
            self.extra.push(format!("{}={:?}", field.name(), value));
        }
    }
}
