use rand_distr::Distribution;
use rand::rngs::StdRng;
use rand::SeedableRng;
use std::cell::RefCell;

thread_local! {
    pub static RNG: RefCell<StdRng> = RefCell::new(SeedableRng::from_seed([0;32]));
}

#[derive(Clone, Copy, Debug)]
pub struct RandomVariable<T> where T: Distribution<f64> {
    pub dist: T,
    pub offset: f64,
}

impl<T> RandomVariable<T> where T: Distribution<f64> {
    pub fn sample(&mut self) -> f64 {
        RNG.with(|rng| {
            self.offset + self.dist.sample(&mut *rng.borrow_mut())
        })
    }
}

pub fn seed(new_seed: u8){
    RNG.with(|rng| {
        rng.replace(SeedableRng::from_seed([new_seed;32]));
    });
}