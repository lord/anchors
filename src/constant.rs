use crate::{Anchor, AnchorInner, Engine, OutputContext, UpdateContext};
use std::panic::Location;
use std::task::Poll;

pub struct Constant<T> {
    val: T,
    first_poll: bool,
    location: &'static Location<'static>,
}

impl<T: 'static> Constant<T> {
    #[track_caller]
    pub fn new<E: Engine>(val: T) -> Anchor<T, E> {
        E::mount(Self {
            val,
            first_poll: true,
            location: Location::caller(),
        })
    }
}

impl<T: 'static, E: Engine> AnchorInner<E> for Constant<T> {
    type Output = T;
    fn dirty(&mut self, _child: &E::AnchorData) {
        panic!("Constant never has any inputs; dirty should not have been called.")
    }
    fn poll_updated<G: UpdateContext<Engine = E>>(&mut self, _ctx: &mut G) -> Poll<bool> {
        let res = Poll::Ready(self.first_poll);
        self.first_poll = false;
        res
    }
    fn output<'slf, 'out, G: OutputContext<'out, Engine = E>>(
        &'slf self,
        _ctx: &mut G,
    ) -> &'out Self::Output
    where
        'slf: 'out,
    {
        &self.val
    }

    fn debug_location(&self) -> Option<(&'static str, &'static Location<'static>)> {
        Some(("constant", self.location))
    }
}
