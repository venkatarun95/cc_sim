use rand::rngs::StdRng;
use rand::SeedableRng;
use std::cell::RefCell;
use serde::{Serialize, Deserialize};
use rand_distr::*;

thread_local! {
    pub static RNG: RefCell<StdRng> = RefCell::new(SeedableRng::from_seed([0;32]));
}

pub fn seed(new_seed: u8){
    RNG.with(|rng| {
        rng.replace(SeedableRng::from_seed([new_seed;32]));
    });
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum CustomDistribution {
    Poisson(f64),
}

impl CustomDistribution {
    pub fn sample(self) -> f64 {
        RNG.with(|rng| {
            let dist = match self {
                CustomDistribution::Poisson(lambda) => rand_distr::Poisson::new(lambda).unwrap()
            };
            dist.sample(&mut *rng.borrow_mut())
        }) 
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct RandomVariable {
    pub dist: CustomDistribution,
    pub offset: f64,
}

impl RandomVariable {
    pub fn sample(&mut self) -> f64 {
        self.offset + self.dist.sample()
    }
}