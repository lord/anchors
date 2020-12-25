use crate::{Anchor, AnchorInner, Engine, OutputContext, Poll, UpdateContext};
use std::panic::Location;

pub struct RefMap<A, F> {
    pub(super) f: F,
    pub(super) anchors: A,
    pub(super) location: &'static Location<'static>,
}

impl<F, In, Out, E> AnchorInner<E> for RefMap<(Anchor<In, E>,), F>
where
    In: AnchorInner<E> + 'static,
    Out: 'static,
    E: Engine,
    F: for<'any> Fn(&'any In::Output) -> &'any Out,
{
    type Output = Out;

    fn dirty(&mut self, _edge: &<E::AnchorHandle as crate::AnchorHandle>::Token) {
        // noop
    }
    fn poll_updated<G: UpdateContext<Engine = E>>(&mut self, ctx: &mut G) -> Poll {
        ctx.request(&self.anchors.0.handle(), true)
    }
    fn output<'slf, 'out, G: OutputContext<'out, Engine = E>>(
        &'slf self,
        ctx: &mut G,
    ) -> &'out Self::Output
    where
        'slf: 'out,
    {
        let val = ctx.get(&self.anchors.0);
        (self.f)(val)
    }

    fn debug_location(&self) -> Option<(&'static str, &'static Location<'static>)> {
        Some(("refmap", self.location))
    }
}
