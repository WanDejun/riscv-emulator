use clap::ValueEnum;
use flexi_logger::{
    Cleanup, Criterion, Duplicate, FileSpec, Logger, LoggerHandle, Naming, WriteMode, opt_format,
};

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl LogLevel {
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
    Logger::try_with_str(level.name())
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
        .format_for_files(opt_format)
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
