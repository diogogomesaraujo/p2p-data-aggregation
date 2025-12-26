use rand::{Rng, SeedableRng, rngs::SmallRng};

pub struct Poisson<R: Rng + ?Sized> {
    pub rng: Box<R>,
    pub rate: f32,
}

impl Poisson<SmallRng> {
    pub fn new(rate: f32, seed: &[u8; 32]) -> Self {
        Self {
            rng: Box::new(SmallRng::from_seed(*seed)),
            rate,
        }
    }
    pub fn time_for_next_event(&mut self) -> f32 {
        -(1.0f32 - self.rng.random::<f32>()).ln() / self.rate
    }
}
