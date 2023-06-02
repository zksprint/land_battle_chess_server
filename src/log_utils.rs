use std::path::PathBuf;

use fern::{
    colors::{Color, ColoredLevelConfig},
    Output,
};
pub fn setup_log_dispatch(log_path: Option<PathBuf>) -> eyre::Result<fern::Dispatch> {
    let mut colors = ColoredLevelConfig::new();
    colors.trace = Color::Cyan;
    colors.debug = Color::Magenta;
    colors.info = Color::Green;
    colors.warn = Color::Red;
    colors.error = Color::BrightRed;

    let log_output: Output = if let Some(log_path) = log_path {
        fern::log_file(log_path)?.into()
    } else {
        std::io::stdout().into()
    };

    // setup logging both to stdout and file
    Ok(fern::Dispatch::new()
        .format(move |out, message, record| {
            out.finish(format_args!(
                "[{}][{}] {}",
                chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
                colors.color(record.level()),
                message
            ))
        })
        .chain(log_output))
}
