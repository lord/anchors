use std::num::NonZeroU64;

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Copy)]
pub struct Generation(NonZeroU64);
impl Generation {
    pub fn new() -> Generation {
        Generation(NonZeroU64::new(1).unwrap())
    }
    pub fn increment(&mut self) {
        let gen: u64 = u64::from(self.0) + 1;
        self.0 = NonZeroU64::new(gen).unwrap();
    }
}
