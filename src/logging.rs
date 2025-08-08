use flexi_logger::{
    Cleanup, Criterion, Duplicate, FileSpec, Logger, LoggerHandle, Naming, WriteMode,
    detailed_format,
};

/// Initialize the logger.
/// Must keep the [`LoggerHandle`] (returned value) alive up to the very end of your program
/// to ensure that all buffered log lines are flushed out.
#[must_use]
pub fn init() -> LoggerHandle {
    Logger::try_with_str("trace")
        .unwrap()
        .log_to_file(
            FileSpec::default()
                .directory("logs")
                .basename("emulator")
                .suffix("log"),
        )
        .rotate(
            Criterion::Size(10_000_000), // 10 MB
            Naming::Numbers,
            Cleanup::KeepLogFiles(3),
        )
        .write_mode(WriteMode::BufferAndFlush)
        .duplicate_to_stderr(Duplicate::Warn)
        .format_for_files(detailed_format)
        .start()
        .unwrap()
}
