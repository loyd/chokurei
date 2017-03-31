use std::env;

use log::{LogRecord, LogLevel, SetLoggerError};
use env_logger::LogBuilder;
use time;

macro_rules! stylish {
    ($style:expr) => (concat!("\x1b[", $style, "m"))
}

fn format(record: &LogRecord) -> String {
    let (shortcut, style) = match record.level() {
        LogLevel::Error => ("ERRO", stylish!("31")),
        LogLevel::Warn  => ("WARN", stylish!("33")),
        LogLevel::Info  => ("INFO", stylish!("32")),
        LogLevel::Debug => ("DEBG", stylish!("34")),
        LogLevel::Trace => ("TRCE", stylish!("35"))
    };

    let timestamp = time::now();

    format!("{st_ts}{timestamp}{st_rst} [{st_lvl}{shortcut}{st_rst}] {message}",
            st_ts = stylish!("37"),
            st_lvl = style,
            st_rst = stylish!("0"),
            timestamp = timestamp.strftime("%d/%m %T").unwrap(),
            shortcut = shortcut,
            message = record.args())
}

pub fn init() -> Result<(), SetLoggerError> {
    let mut builder = LogBuilder::new();

    builder.format(format);

    if let Ok(s) = env::var("RUST_LOG") {
        builder.parse(&s);
    }

    builder.init()
}
