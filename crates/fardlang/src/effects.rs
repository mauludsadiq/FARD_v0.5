use valuecore::v0::V;
use anyhow::Result;

#[derive(Debug, Clone)]
pub struct EffectTrace {
    pub name: String,
    pub args: Vec<V>,
    pub result: V,
    pub timestamp_ms: u64,
}

pub trait EffectHandler {
    fn call(&mut self, name: &str, args: &[V]) -> Result<V>;
    fn trace(&self) -> &[EffectTrace];
}
