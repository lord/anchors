use crate::{Anchor, AnchorHandle, AnchorInner, Engine, OutputContext, Poll, UpdateContext};
use std::panic::Location;

pub struct Then<A, Out, F, E: Engine> {
    pub(super) f: F,
    pub(super) f_anchor: Option<Anchor<Out, E>>,
    pub(super) lhs_stale: bool,
    pub(super) anchors: A,
    pub(super) location: &'static Location<'static>,
}

macro_rules! impl_tuple_then {
    ($([$output_type:ident, $num:tt])+) => {
        impl<$($output_type,)+ E, F, Out> AnchorInner<E> for
            Then<( $(Anchor<$output_type, E>,)+ ), Out, F, E>
        where
            F: for<'any> FnMut($(&'any $output_type),+) -> Anchor<Out, E>,
            Out: 'static,
            $(
                $output_type: 'static,
            )+
            E: Engine,
        {
            type Output = Out;
            fn dirty(&mut self, edge: &<E::AnchorHandle as AnchorHandle>::Token) {
                $(
                    // only invalidate f_anchor if one of the lhs anchors is invalidated
                    if edge == &self.anchors.$num.data.token() {
                        self.lhs_stale = true;
                        return;
                    }
                )+
            }
            fn poll_updated<G: UpdateContext<Engine=E>>(
                &mut self,
                ctx: &mut G,
            ) -> Poll {
                if self.f_anchor.is_none() || self.lhs_stale {
                    let mut found_pending = false;
                    let mut found_updated = false;

                    $(
                        match ctx.request(&self.anchors.$num, true) {
                            Poll::Pending => {
                                found_pending = true;
                            }
                            Poll::Updated => {
                                found_updated = true;
                            }
                            Poll::Unchanged => {
                                // do nothing
                            }
                        }
                    )+

                    if found_pending {
                        return Poll::Pending;
                    }

                    self.lhs_stale = false;

                    if self.f_anchor.is_none() || found_updated {
                        let new_anchor = (self.f)($(&ctx.get(&self.anchors.$num)),+);
                        match self.f_anchor.as_ref() {
                            Some(outdated_anchor) if outdated_anchor != &new_anchor => {
                                // changed, so unfollow old
                                ctx.unrequest(outdated_anchor);
                            }
                            _ => {
                            }
                        }
                        self.f_anchor = Some(new_anchor);
                    }
                }

                ctx.request(&self.f_anchor.as_ref().unwrap(), true)
            }
            fn output<'slf, 'out, G: OutputContext<'out, Engine=E>>(
                &'slf self,
                ctx: &mut G,
            ) -> &'out Self::Output
            where
                'slf: 'out,
            {
                &ctx.get(&self.f_anchor.as_ref().unwrap())
            }

            fn debug_location(&self) -> Option<(&'static str, &'static Location<'static>)> {
                Some(("then", self.location))
            }
        }
    }
}

impl_tuple_then! {
    [O0, 0]
}

impl_tuple_then! {
    [O0, 0]
    [O1, 1]
}

impl_tuple_then! {
    [O0, 0]
    [O1, 1]
    [O2, 2]
}

impl_tuple_then! {
    [O0, 0]
    [O1, 1]
    [O2, 2]
    [O3, 3]
}

impl_tuple_then! {
    [O0, 0]
    [O1, 1]
    [O2, 2]
    [O3, 3]
    [O4, 4]
}

impl_tuple_then! {
    [O0, 0]
    [O1, 1]
    [O2, 2]
    [O3, 3]
    [O4, 4]
    [O5, 5]
}

impl_tuple_then! {
    [O0, 0]
    [O1, 1]
    [O2, 2]
    [O3, 3]
    [O4, 4]
    [O5, 5]
    [O6, 6]
}

impl_tuple_then! {
    [O0, 0]
    [O1, 1]
    [O2, 2]
    [O3, 3]
    [O4, 4]
    [O5, 5]
    [O6, 6]
    [O7, 7]
}

impl_tuple_then! {
    [O0, 0]
    [O1, 1]
    [O2, 2]
    [O3, 3]
    [O4, 4]
    [O5, 5]
    [O6, 6]
    [O7, 7]
    [O8, 8]
}
