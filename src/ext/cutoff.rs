use crate::{Anchor, AnchorHandle, AnchorInner, Engine, OutputContext, Poll, UpdateContext};
use std::panic::Location;

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

    fn dirty(&mut self, _edge: &<E::AnchorHandle as AnchorHandle>::Token) {
        // noop
    }
    fn poll_updated<G: UpdateContext<Engine = E>>(&mut self, ctx: &mut G) -> Poll {
        let upstream_poll = ctx.request(&self.anchors.0, true);
        if upstream_poll != Poll::Updated {
            return upstream_poll;
        }

        let val = ctx.get(&self.anchors.0);
        if (self.f)(val) {
            Poll::Updated
        } else {
            Poll::Unchanged
        }
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
