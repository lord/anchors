use crate::{Anchor, AnchorInner, Engine, OutputContext, Poll, UpdateContext};
use std::panic::Location;

pub struct Map<A, F, Out> {
    pub(super) f: F,
    pub(super) output: Option<Out>,
    pub(super) output_stale: bool,
    pub(super) anchors: A,
    pub(super) location: &'static Location<'static>,
}

macro_rules! impl_tuple_map {
    ($([$output_type:ident, $num:tt])+) => {
        impl<$($output_type,)+ E, F, Out> AnchorInner<E> for
            Map<($(Anchor<$output_type, E>,)+), F, Out>
        where
            F: for<'any> FnMut($(&'any $output_type),+) -> Out,
            Out: 'static,
            $(
                $output_type: 'static,
            )+
            E: Engine,
        {
            type Output = Out;
            fn dirty(&mut self, _edge:  &<E::AnchorHandle as crate::AnchorHandle>::Token) {
                println!("map dirtied");
                self.output_stale = true;
            }
            fn poll_updated<G: UpdateContext<Engine=E>>(
                &mut self,
                ctx: &mut G,
            ) -> Poll {
                if !self.output_stale && self.output.is_some() {
                    return Poll::Unchanged;
                }

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

                self.output_stale = false;

                if self.output.is_none() || found_updated {
                    let new_val = (self.f)($(&ctx.get(&self.anchors.$num)),+);
                    self.output = Some(new_val);
                    Poll::Updated
                } else {
                    Poll::Unchanged
                }
            }
            fn output<'slf, 'out, G: OutputContext<'out, Engine=E>>(
                &'slf self,
                _ctx: &mut G,
            ) -> &'out Self::Output
            where
                'slf: 'out,
            {
                self.output
                    .as_ref()
                    .expect("output called on Map before value was calculated")
            }

            fn debug_location(&self) -> Option<(&'static str, &'static Location<'static>)> {
                Some(("map", self.location))
            }
        }
    }
}

impl_tuple_map! {
    [O0, 0]
}

impl_tuple_map! {
    [O0, 0]
    [O1, 1]
}

impl_tuple_map! {
    [O0, 0]
    [O1, 1]
    [O2, 2]
}

impl_tuple_map! {
    [O0, 0]
    [O1, 1]
    [O2, 2]
    [O3, 3]
}

impl_tuple_map! {
    [O0, 0]
    [O1, 1]
    [O2, 2]
    [O3, 3]
    [O4, 4]
}

impl_tuple_map! {
    [O0, 0]
    [O1, 1]
    [O2, 2]
    [O3, 3]
    [O4, 4]
    [O5, 5]
}

impl_tuple_map! {
    [O0, 0]
    [O1, 1]
    [O2, 2]
    [O3, 3]
    [O4, 4]
    [O5, 5]
    [O6, 6]
}

impl_tuple_map! {
    [O0, 0]
    [O1, 1]
    [O2, 2]
    [O3, 3]
    [O4, 4]
    [O5, 5]
    [O6, 6]
    [O7, 7]
}

impl_tuple_map! {
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
