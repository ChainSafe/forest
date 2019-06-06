use logging_toolkit::make_logger;
use slog::Logger;

lazy_static! {
    pub static ref FCP_LOG: Logger = make_logger("sector-builder");
}
