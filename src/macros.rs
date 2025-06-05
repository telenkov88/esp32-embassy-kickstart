#[macro_export]
macro_rules! try_log {
    ($expr:expr, $context:literal) => {
        if let Err(e) = $expr {
            error!(concat!("NeoPixel error (", $context, "): {:?}"), e);
        }
    };
}
