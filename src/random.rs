use rand::rngs::StdRng;
use rand::SeedableRng;
use rand_distr::{Distribution, Exp};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;

thread_local! {
    pub static RNG: RefCell<StdRng> = RefCell::new(SeedableRng::from_seed([0;32]));
}

pub fn seed(new_seed: u8) {
    RNG.with(|rng| {
        rng.replace(SeedableRng::from_seed([new_seed; 32]));
    });
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum RandomVariable {
    /// Not really random. Returns the given value every time
    Const(f64),
    /// Exponential distribution with given rate (1 / mean)
    Exponential(f64),
}

impl RandomVariable {
    pub fn sample(&self) -> f64 {
        RNG.with(|rng| {
            let rng = &mut *rng.borrow_mut();
            match self {
                Self::Const(val) => *val,
                Self::Exponential(lambda) => Exp::new(*lambda).unwrap().sample(rng),
            }
        })
    }
}
