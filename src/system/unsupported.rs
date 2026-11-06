use crate::config::Action;

pub fn init_thread() {}
pub fn volume_backend() -> String {
    "não suportado neste sistema".into()
}
pub fn get_volume() -> Result<f32, String> {
    Err("Controlo de volume não suportado neste sistema".into())
}
pub fn set_volume(_v: f32) -> Result<(), String> {
    Err("Controlo de volume não suportado neste sistema".into())
}
pub fn power_action(_a: Action, _force: bool) -> Result<(), String> {
    Err("Ação não suportada neste sistema".into())
}
