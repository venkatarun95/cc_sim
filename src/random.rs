use rand_distr::Distribution;
use rand::Rng;

#[derive(Clone, Copy, Debug)]
pub struct RandomVariable<T, U> where T: Distribution<f64>, U: Rng {
    pub dist: T,
    pub rng: U,
}

impl<T, U> RandomVariable<T, U> where T: Distribution<f64>, U: Rng {
    pub fn sample(&mut self) -> f64 {
        return self.dist.sample(&mut self.rng);
    }
}
