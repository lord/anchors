use crate::{Anchor, AnchorInner, Engine, OutputContext, UpdateContext};
use std::panic::Location;
use std::task::Poll;

pub struct RefMap<A, F> {
    pub(super) f: F,
    pub(super) anchors: A,
    pub(super) location: &'static Location<'static>,
}

impl <F, In: 'static, Out: 'static, E> AnchorInner<E> for RefMap<(Anchor<In, E>,), F>
where
    E: Engine,
    F: for <'any> Fn(&'any In) -> &'any Out,
{
    type Output = Out;

    fn dirty(&mut self, _edge: &E::AnchorData) {
        // noop
    }
    fn poll_updated<G: UpdateContext<Engine=E>>(
        &mut self,
        ctx: &mut G,
    ) -> Poll<bool> {
        let mut found_pending = false;

        if ctx.request(&self.anchors.0, true).is_pending() {
            found_pending = true;
        }

        if found_pending {
            return Poll::Pending;
        }

        // TODO fix always marking as dirty
        Poll::Ready(true)
    }

    fn output<'slf, 'out, G: OutputContext<'out, Engine=E>>(
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
