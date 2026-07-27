# valence-telemetry

`TelemetrySink` trait with `NoOpSink`, `ConsoleSink`, and `RecordingSink`. `NoOpSink` is the default via builder; inject custom sinks from separate adapter crates at boot. Product telemetry stays out of this crate.
