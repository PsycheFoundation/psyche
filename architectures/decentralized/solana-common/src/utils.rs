pub fn ui_amount_to_native_amount(ui_amount: f64, decimals: u8) -> u64 {
    let factor = 10u64.pow(decimals as u32) as f64;
    (ui_amount * factor).round() as u64
}

pub fn native_amount_to_ui_amount(native_amount: u64, decimals: u8) -> f64 {
    let factor = 10u64.pow(decimals as u32) as f64;
    (native_amount as f64) / factor
}
