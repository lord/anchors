use crate::{Engine, AnchorHandle, OutputContext, UpdateContext, Poll};

pub trait AnchorInner<'a, E: Engine + ?Sized> {
    type Output: 'a;

    /// Called by the engine to indicate some input may have changed.
    /// If this `AnchorInner` still cares about `child`'s value, it should re-request
    /// it next time `poll_updated` is called.
    fn dirty(&mut self, child: &<E::AnchorHandle as AnchorHandle>::Token);

    /// Called by the engine when it wants to know if this value has changed or
    /// not. If some requested value from `ctx` is `Pending`, this method should
    /// return `Poll::Pending`; otherwise it must finish recalculation and report
    /// either `Poll::Updated` or `Poll::Unchanged`.
    fn poll_updated<G: UpdateContext<Engine = E>>(&mut self, ctx: &mut G) -> Poll;

    /// Called by the engine to get the current output value of this `AnchorInner`. This
    /// is *only* called after this `AnchorInner` reported in the return value from
    /// `poll_updated` the value was ready. If `dirty` is called, this function will not
    /// be called until `poll_updated` returns a non-Pending value.
    fn output<G: OutputContext<'a, Engine = E>>(
        &'a self,
        ctx: &mut G,
    ) -> Self::Output;
}
