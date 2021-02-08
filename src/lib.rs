mod prelude;

#[macro_use]
pub mod macros;

pub mod benchmarks;
pub mod runner;
pub mod util;

#[macro_use]
extern crate lazy_static;

// use crate::prelude::*;

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
