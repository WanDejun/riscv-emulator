use clap::ValueEnum;
use flexi_logger::{
    Cleanup, Criterion, Duplicate, FileSpec, LogSpecBuilder, Logger, LoggerHandle, Naming,
    WriteMode, default_format,
};
use log::LevelFilter;

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl LogLevel {
    pub fn to_level_filter(&self) -> LevelFilter {
        match self {
            LogLevel::Trace => LevelFilter::Trace,
            LogLevel::Debug => LevelFilter::Debug,
            LogLevel::Info => LevelFilter::Info,
            LogLevel::Warn => LevelFilter::Warn,
            LogLevel::Error => LevelFilter::Error,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            LogLevel::Trace => "trace",
            LogLevel::Debug => "debug",
            LogLevel::Info => "info",
            LogLevel::Warn => "warn",
            LogLevel::Error => "error",
        }
    }
}

/// Initialize the logger.
/// Must keep the [`LoggerHandle`] (returned value) alive up to the very end of your program
/// to ensure that all buffered log lines are flushed out.
#[must_use]
pub fn init(level: LogLevel) -> LoggerHandle {
    let mut builder = LogSpecBuilder::new();
    builder.module("rustyline", log::LevelFilter::Warn);
    builder.default(level.to_level_filter());

    Logger::with(builder.build())
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
        .duplicate_to_stderr(Duplicate::Error)
        .format_for_files(default_format)
        .start()
        .unwrap()
}

#[macro_export]
macro_rules! error_and_panic {
    ($($arg:tt)*) => {
        {
            log::error!($($arg)*);
            panic!();
        }
    };
}
