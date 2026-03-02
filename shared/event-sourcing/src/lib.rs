pub mod coordinator_source;
pub mod events;
pub mod projection;
pub mod store;
pub mod timeline;
pub mod tracing_layer;

pub use events::*;
pub use store::{Backend, EventStore, FileBackend, InMemoryBackend};
pub use tracing_layer::EventStoreTracingLayer;

pub use chrono::Utc;

/// Emit an event to the global event store
#[macro_export]
macro_rules! event {
    // event!(whatever, timestamp)
    ($event:expr, $timestamp:expr) => {{
        #[allow(unused_imports)]
        use $crate::events::*;
        $crate::EventStore::emit(($event).into(), $timestamp);
    }};

    // event!(whatever)
    ($event:expr) => {{
        #[allow(unused_imports)]
        use $crate::events::*;
        $crate::EventStore::emit(($event).into(), $crate::Utc::now());
    }};
}
