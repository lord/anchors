use crate::{Anchor, AnchorInner, Engine, OutputContext, UpdateContext};
use std::panic::Location;
use std::task::Poll;

pub struct Cutoff<A, F> {
    pub(super) f: F,
    pub(super) anchors: A,
    pub(super) location: &'static Location<'static>,
}

impl<F, In: 'static, E> AnchorInner<E> for Cutoff<(Anchor<In, E>,), F>
where
    E: Engine,
    F: for<'any> FnMut(&'any In) -> bool,
{
    type Output = In;

    fn dirty(&mut self, _edge: &E::AnchorData) {
        // noop
    }
    fn poll_updated<G: UpdateContext<Engine = E>>(&mut self, ctx: &mut G) -> Poll<bool> {
        let mut found_pending = false;

        if ctx.request(&self.anchors.0, true).is_pending() {
            found_pending = true;
        }

        if found_pending {
            return Poll::Pending;
        }

        let val = ctx.get(&self.anchors.0);

        Poll::Ready((self.f)(val))
    }

    fn output<'slf, 'out, G: OutputContext<'out, Engine = E>>(
        &'slf self,
        ctx: &mut G,
    ) -> &'out Self::Output
    where
        'slf: 'out,
    {
        ctx.get(&self.anchors.0)
    }

    fn debug_location(&self) -> Option<(&'static str, &'static Location<'static>)> {
        Some(("cutoff", self.location))
    }
}
