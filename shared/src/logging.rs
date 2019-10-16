use std::error::Error;
use std::thread;

use fern;
use fern::colors::{Color, ColoredLevelConfig};

pub fn setup() -> Result<(), Box<dyn Error + Sync + Send>> {
    let colors = ColoredLevelConfig::new().error(Color::Red).warn(Color::Yellow).info(Color::Green).debug(Color::Magenta).trace(Color::BrightBlack);

    fern::Dispatch::new()
        .format(move |out, msg, record| {
            out.finish(format_args!(
                "{color}{file}:{line} | {thread} | {level} | {message}\x1B[0m",
                color = format_args!("\x1B[{}m", colors.get_color(&record.level()).to_fg_str()),
                file = record.file().unwrap_or("unknown"),
                line = record.line().unwrap_or(0),
                thread = thread::current().name().unwrap_or("unknown"),
                level = record.level(),
                message = msg
            ))
        }).chain(std::io::stdout()).apply()?;

    Ok(())
}
