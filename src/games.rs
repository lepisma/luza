use anyhow::Result;

pub mod azul;

pub trait Validate {
    fn validate(&self) -> Result<()>;
}

pub trait Representable {
    fn represent(&self) -> Vec<f64>;
}
