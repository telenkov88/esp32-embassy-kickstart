pub const fn or_str(opt: Option<&'static str>, default: &'static str) -> &'static str {
    if let Some(val) = opt {
        val
    } else if opt.is_none() {
        default
    } else {
        unreachable!()
    }
}
