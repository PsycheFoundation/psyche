# event-sourcing

structured event logging for psyche runs. uses postcard for compact serialization and COBS framing for crash-safe streaming writes to disk.

## usage

```rust
use psyche_event_sourcing::*;

// init with backends you want
EventStore::init(vec![
    Box::new(InMemoryBackend::default()),  // for tests/debugging
    Box::new(FileBackend::new(
        Path::new("/data/events"),
        0,  // initial epoch
        RunStarted {
            run_id: "run-123".into(),
            node_id: "node-1".into(), // probably pubkey
            config: "abc123".into(), // this should have args like dp/tp/whatever. maybe structure it?
            psyche_version: "0.1.0".into(), // gitcommit / docker image sha256
        }
    )?),
]);

// emit events with tags to link em
event!(train::TrainingStarted, [Tag::BatchId(batch_id));
event!(train::TrainingFinished, [Tag::BatchId(batch_id)]);

// rotate to new file after cooldown
event!(EpochStarted { epoch_number: 1 });
```

## backends

InMemoryBackend keeps events in a vec, useful for tests

FileBackend streams to disk with an async actor, auto-rotates files on each `EpochStarted` event

files are named `events-epoch-{n}-{timestamp}.postcard` and each is self-contained (includes RunStarted + EpochStarted).

## reading events back

```rust
// import from file into InMemoryBackend
EventStore::init(vec![Box::new(InMemoryBackend::default())]);
EventStore::import_streamed_file("events-epoch-0-2026-02-13T18-30-45Z.postcard")?;

// access the backend
EventStore::with_backend::<InMemoryBackend, _, _>(|b| {
    for event in b.events() {
        println!("{:?}", event);
    }
});
```

## tags

use tags to link related events:

- `BlobUpload(id)` + `Blob(hash)` for uploads
- `BatchId(id)` for training
- `EventSubmission(id)` for coordinator writes

## crash safety

COBS framing means partial writes from crashes get detected and skipped on import. earlier events are never corrupted.
